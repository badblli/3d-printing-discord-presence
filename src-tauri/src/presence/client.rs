//! Bounded local IPC, latest-value coalescing and reconnect scheduling.
use super::{valid_app_id, Activity};
use async_trait::async_trait;
use serde_json::{json, Value};
#[cfg(any(unix, test))]
use std::path::PathBuf;
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::watch,
    task::JoinHandle,
    time::{timeout, Instant},
};

const INTERVAL: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_FRAME: usize = 1024 * 1024;
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenceState {
    Disconnected,
    Connected,
    Error(&'static str),
}

/// Owns a background Tokio task. Construct inside a Tokio runtime; `set` only
/// replaces the desired value and never blocks on Discord. Pending updates and
/// reconnects run even when the caller stops submitting telemetry. Dropping this
/// handle closes IPC and stops the task (Discord removes this process's card).
pub struct PresenceClient {
    desired: watch::Sender<Option<Activity>>,
    states: watch::Sender<PresenceState>,
    task: JoinHandle<()>,
}

impl PresenceClient {
    pub fn new(application_id: &str) -> std::result::Result<Self, PresenceState> {
        if !valid_app_id(application_id) {
            return Err(PresenceState::Error("invalid application id"));
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| PresenceState::Error("async runtime unavailable"))?;
        let (desired, changes) = watch::channel(None);
        let (states, _) = watch::channel(PresenceState::Disconnected);
        let task = runtime.spawn(run(
            Driver::new(application_id.into(), IpcTransport::default()),
            changes,
            states.clone(),
        ));
        Ok(Self {
            desired,
            states,
            task,
        })
    }

    pub fn set(&self, activity: Option<Activity>) {
        self.desired.send_if_modified(|current| {
            if *current == activity {
                false
            } else {
                *current = activity;
                true
            }
        });
    }

    /// Latest state plus a notification whenever it changes; bodies and remote
    /// error messages are deliberately never exposed.
    pub fn subscribe(&self) -> watch::Receiver<PresenceState> {
        self.states.subscribe()
    }
}

impl Drop for PresenceClient {
    fn drop(&mut self) {
        self.task.abort();
        self.states.send_replace(PresenceState::Disconnected);
    }
}

#[derive(Debug, PartialEq)]
struct Frame {
    opcode: u32,
    body: Value,
}

fn encode_frame(opcode: u32, body: &Value) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(body).map_err(|_| "invalid IPC payload")?;
    if payload.len() > MAX_FRAME || opcode > 4 {
        return Err("invalid IPC frame");
    }
    let mut bytes = Vec::with_capacity(payload.len() + 8);
    bytes.extend_from_slice(&opcode.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

/// A partial frame stays in the transport across cancelled receive futures.
fn decode_frame(bytes: &mut Vec<u8>) -> Result<Option<Frame>> {
    if bytes.len() < 8 {
        return Ok(None);
    }
    let opcode = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "invalid IPC header")?);
    let length =
        u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| "invalid IPC header")?) as usize;
    if length > MAX_FRAME || opcode > 4 {
        return Err("invalid IPC frame");
    }
    if bytes.len() < length + 8 {
        return Ok(None);
    }
    let body = serde_json::from_slice(&bytes[8..8 + length]).map_err(|_| "invalid IPC JSON")?;
    bytes.drain(..8 + length);
    Ok(Some(Frame { opcode, body }))
}

#[async_trait]
trait Transport: Send {
    async fn connect(&mut self) -> Result<()>;
    async fn send(&mut self, opcode: u32, body: &Value) -> Result<()>;
    /// Must be cancellation safe: the worker also waits for updates and deadlines.
    async fn receive(&mut self) -> Result<Frame>;
    fn disconnect(&mut self);
}

trait IpcStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> IpcStream for T {}

#[derive(Default)]
struct IpcTransport {
    stream: Option<Box<dyn IpcStream>>,
    incoming: Vec<u8>,
}

// Also compiled on Windows so all endpoint combinations can be unit-tested.
#[cfg(any(unix, test))]
fn unix_paths(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in roots {
        if root.as_os_str().is_empty() {
            continue;
        }
        for subpath in ["", "app/com.discordapp.Discord", "snap.discord"] {
            for index in 0..10 {
                let path = root.join(subpath).join(format!("discord-ipc-{index}"));
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

#[async_trait]
impl Transport for IpcTransport {
    async fn connect(&mut self) -> Result<()> {
        self.disconnect();
        #[cfg(windows)]
        for index in 0..10 {
            if let Ok(stream) = tokio::net::windows::named_pipe::ClientOptions::new()
                .open(format!(r"\\.\pipe\discord-ipc-{index}"))
            {
                self.stream = Some(Box::new(stream));
                return Ok(());
            }
        }
        #[cfg(unix)]
        {
            let mut roots: Vec<_> = ["XDG_RUNTIME_DIR", "TMPDIR"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(PathBuf::from)
                .collect();
            roots.push(PathBuf::from("/tmp"));
            for path in unix_paths(roots) {
                if let Ok(stream) = tokio::net::UnixStream::connect(path).await {
                    self.stream = Some(Box::new(stream));
                    return Ok(());
                }
            }
        }
        Err("Discord IPC unavailable")
    }

    async fn send(&mut self, opcode: u32, body: &Value) -> Result<()> {
        let bytes = encode_frame(opcode, body)?;
        self.stream
            .as_mut()
            .ok_or("Discord IPC disconnected")?
            .write_all(&bytes)
            .await
            .map_err(|_| "Discord IPC write failed")
    }

    async fn receive(&mut self) -> Result<Frame> {
        loop {
            if let Some(frame) = decode_frame(&mut self.incoming)? {
                return Ok(frame);
            }
            let mut chunk = [0; 8192];
            let count = self
                .stream
                .as_mut()
                .ok_or("Discord IPC disconnected")?
                .read(&mut chunk)
                .await
                .map_err(|_| "Discord IPC read failed")?;
            if count == 0 {
                return Err("Discord IPC disconnected");
            }
            self.incoming.extend_from_slice(&chunk[..count]);
        }
    }

    fn disconnect(&mut self) {
        self.stream = None;
        self.incoming.clear();
    }
}

struct Driver<T: Transport> {
    application_id: String,
    transport: T,
    desired: Option<Activity>,
    // Outer None means no successful send on this connection, inner None a clear.
    sent: Option<Option<Activity>>,
    last_send: Option<Instant>,
    retry_at: Instant,
    state: PresenceState,
}

impl<T: Transport> Driver<T> {
    fn new(application_id: String, transport: T) -> Self {
        Self {
            application_id,
            transport,
            desired: None,
            sent: None,
            last_send: None,
            retry_at: Instant::now(),
            state: PresenceState::Disconnected,
        }
    }

    fn fail(&mut self, reason: &'static str, now: Instant) {
        self.transport.disconnect();
        self.sent = None;
        self.state = PresenceState::Error(reason);
        self.retry_at = now + INTERVAL;
    }

    async fn handshake(&mut self) -> Result<()> {
        self.transport.connect().await?;
        self.transport
            .send(0, &json!({ "v": 1, "client_id": self.application_id }))
            .await?;
        loop {
            let frame = self.transport.receive().await?;
            match frame.opcode {
                1 if frame.body.get("evt").and_then(Value::as_str) == Some("READY") => {
                    return Ok(())
                }
                3 => self.transport.send(4, &frame.body).await?,
                _ => return Err("Discord IPC handshake rejected"),
            }
        }
    }

    async fn tick(&mut self, now: Instant) {
        if self.state != PresenceState::Connected {
            if now < self.retry_at {
                return;
            }
            let result = timeout(IO_TIMEOUT, self.handshake())
                .await
                .unwrap_or(Err("Discord IPC handshake timed out"));
            if let Err(reason) = result {
                self.fail(reason, now.max(Instant::now()));
                return;
            }
            self.state = PresenceState::Connected;
        }
        if self.sent.as_ref() == Some(&self.desired)
            || self
                .last_send
                .is_some_and(|last| now.saturating_duration_since(last) < INTERVAL)
        {
            return;
        }
        let body = json!({ "cmd": "SET_ACTIVITY", "args": {
            "pid": std::process::id(), "activity": self.desired,
        }, "nonce": uuid::Uuid::new_v4().to_string() });
        match timeout(IO_TIMEOUT, self.transport.send(1, &body))
            .await
            .unwrap_or(Err("Discord IPC write timed out"))
        {
            Ok(()) => {
                self.sent = Some(self.desired.clone());
                self.last_send = Some(now.max(Instant::now()));
            }
            Err(reason) => self.fail(reason, now.max(Instant::now())),
        }
    }

    fn next_wake(&self) -> Instant {
        if self.state != PresenceState::Connected {
            return self.retry_at;
        }
        if self.sent.as_ref() != Some(&self.desired) {
            return self
                .last_send
                .map_or_else(Instant::now, |last| last + INTERVAL);
        }
        Instant::now() + Duration::from_secs(86_400)
    }

    async fn handle_frame(&mut self, frame: Result<Frame>) {
        let result = match frame {
            Ok(Frame { opcode: 3, body }) => timeout(IO_TIMEOUT, self.transport.send(4, &body))
                .await
                .unwrap_or(Err("Discord IPC write timed out")),
            Ok(Frame { opcode: 1, body })
                if body.get("evt").and_then(Value::as_str) != Some("ERROR") =>
            {
                Ok(())
            }
            Ok(Frame { opcode: 4, .. }) => Ok(()),
            Ok(_) => Err("Discord IPC request rejected"),
            Err(reason) => Err(reason),
        };
        if let Err(reason) = result {
            self.fail(reason, Instant::now());
        }
    }
}

async fn run<T: Transport>(
    mut driver: Driver<T>,
    mut changes: watch::Receiver<Option<Activity>>,
    states: watch::Sender<PresenceState>,
) {
    loop {
        driver.desired = changes.borrow_and_update().clone();
        driver.tick(Instant::now()).await;
        states.send_if_modified(|state| {
            if *state == driver.state {
                false
            } else {
                *state = driver.state;
                true
            }
        });
        let deadline = driver.next_wake();
        tokio::select! {
            changed = changes.changed() => if changed.is_err() { break; },
            _ = tokio::time::sleep_until(deadline) => {},
            frame = driver.transport.receive(), if driver.state == PresenceState::Connected => {
                driver.handle_frame(frame).await;
            }
        }
    }
    driver.transport.disconnect();
    states.send_replace(PresenceState::Disconnected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::activity::{Assets, Timestamps};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeTransport {
        connections: usize,
        disconnects: usize,
        writes: Vec<Frame>,
        fail_connect: bool,
        fail_write: bool,
        replies: VecDeque<Frame>,
        observed: Option<tokio::sync::mpsc::UnboundedSender<Frame>>,
    }
    #[async_trait]
    impl Transport for FakeTransport {
        async fn connect(&mut self) -> Result<()> {
            self.connections += 1;
            if self.fail_connect {
                Err("Discord IPC unavailable")
            } else {
                Ok(())
            }
        }
        async fn send(&mut self, opcode: u32, body: &Value) -> Result<()> {
            if self.fail_write {
                return Err("Discord IPC write failed");
            }
            self.writes.push(Frame {
                opcode,
                body: body.clone(),
            });
            if let Some(observed) = &self.observed {
                let _ = observed.send(Frame {
                    opcode,
                    body: body.clone(),
                });
            }
            Ok(())
        }
        async fn receive(&mut self) -> Result<Frame> {
            match self.replies.pop_front() {
                Some(frame) => Ok(frame),
                None => std::future::pending().await,
            }
        }
        fn disconnect(&mut self) {
            self.disconnects += 1;
        }
    }
    fn ready() -> Frame {
        Frame {
            opcode: 1,
            body: json!({"cmd": "DISPATCH", "evt": "READY"}),
        }
    }
    fn driver() -> Driver<FakeTransport> {
        let mut transport = FakeTransport::default();
        transport.replies.push_back(ready());
        Driver::new("123456789012345678".into(), transport)
    }
    fn activity(percent: u8) -> Activity {
        Activity {
            details: "fake-test".into(),
            state: format!("Test Printer · %{percent}"),
            assets: Assets {
                large_image: "printer".into(),
                large_text: "Test Printer".into(),
                small_image: None,
                small_text: None,
            },
            buttons: Vec::new(),
            timestamps: Some(Timestamps { end: 1120 }),
        }
    }
    fn updates(driver: &Driver<FakeTransport>) -> Vec<&Value> {
        driver
            .transport
            .writes
            .iter()
            .filter(|f| f.opcode == 1)
            .map(|f| &f.body)
            .collect()
    }
    #[test]
    fn frame_round_trip_and_little_endian_header() {
        let body = json!({"fake": "örnek"});
        let mut bytes = encode_frame(1, &body).unwrap();
        assert_eq!(&bytes[..4], &[1, 0, 0, 0]);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8
        );
        assert_eq!(
            decode_frame(&mut bytes).unwrap(),
            Some(Frame { opcode: 1, body })
        );
        assert!(bytes.is_empty());
    }
    #[test]
    fn fragmented_and_coalesced_frames() {
        let frame = encode_frame(3, &json!({"fake": true})).unwrap();
        let mut bytes = Vec::new();
        for byte in &frame[..frame.len() - 1] {
            bytes.push(*byte);
            assert_eq!(decode_frame(&mut bytes).unwrap(), None);
        }
        bytes.push(*frame.last().unwrap());
        bytes.extend_from_slice(&frame);
        assert!(decode_frame(&mut bytes).unwrap().is_some());
        assert!(decode_frame(&mut bytes).unwrap().is_some());
        assert!(bytes.is_empty());
    }
    #[test]
    fn invalid_frames_are_rejected_without_exposing_body() {
        let mut bytes = vec![1, 0, 0, 0, 255, 255, 255, 127];
        assert_eq!(decode_frame(&mut bytes), Err("invalid IPC frame"));
        let mut bytes = vec![1, 0, 0, 0, 1, 0, 0, 0, b'!'];
        assert_eq!(decode_frame(&mut bytes), Err("invalid IPC JSON"));
        assert_eq!(encode_frame(5, &json!({})), Err("invalid IPC frame"));
    }
    #[tokio::test]
    async fn duplicate_activity_is_not_sent_again() {
        let mut driver = driver();
        let now = driver.retry_at;
        driver.desired = Some(activity(42));
        driver.tick(now).await;
        driver.tick(now + INTERVAL * 2).await;
        assert_eq!(updates(&driver).len(), 1);
        assert_eq!(driver.state, PresenceState::Connected);
        assert_eq!(
            driver.transport.writes[0].body,
            json!({"v":1,"client_id":"123456789012345678"})
        );
        assert_eq!(updates(&driver)[0]["cmd"], "SET_ACTIVITY");
        assert_eq!(
            updates(&driver)[0]["args"]["activity"]["timestamps"]["end"],
            1120
        );
        assert!(updates(&driver)[0]["nonce"].is_string());
    }
    #[tokio::test]
    async fn fifteen_second_limit_coalesces_latest_value() {
        let mut driver = driver();
        let now = driver.retry_at;
        driver.desired = Some(activity(1));
        driver.tick(now).await;
        let sent_at = driver.last_send.unwrap();
        driver.desired = Some(activity(2));
        driver.tick(sent_at + Duration::from_secs(14)).await;
        driver.desired = Some(activity(3));
        assert_eq!(updates(&driver).len(), 1);
        assert_eq!(driver.next_wake(), sent_at + INTERVAL);
        driver.tick(sent_at + INTERVAL).await;
        assert_eq!(updates(&driver).len(), 2);
        assert_eq!(
            updates(&driver)[1]["args"]["activity"]["state"],
            "Test Printer · %3"
        );
        assert_ne!(updates(&driver)[0]["nonce"], updates(&driver)[1]["nonce"]);
    }
    #[tokio::test]
    async fn none_clears_once_and_obeys_rate_limit() {
        let mut driver = driver();
        driver.desired = Some(activity(1));
        driver.tick(driver.retry_at).await;
        let sent_at = driver.last_send.unwrap();
        driver.desired = None;
        driver.tick(sent_at + Duration::from_secs(1)).await;
        assert_eq!(updates(&driver).len(), 1);
        driver.tick(sent_at + INTERVAL).await;
        driver.tick(sent_at + INTERVAL * 3).await;
        assert_eq!(updates(&driver).len(), 2);
        assert!(updates(&driver)[1]["args"]["activity"].is_null());
    }
    #[tokio::test]
    async fn initial_none_clears_only_once() {
        let mut driver = driver();
        let now = driver.retry_at;
        driver.tick(now).await;
        driver.tick(now + INTERVAL * 2).await;
        assert_eq!(updates(&driver).len(), 1);
        assert!(updates(&driver)[0]["args"]["activity"].is_null());
    }
    #[tokio::test]
    async fn unavailable_discord_retries_after_fifteen_seconds() {
        let mut driver = driver();
        driver.transport.fail_connect = true;
        driver.desired = Some(activity(42));
        driver.tick(driver.retry_at).await;
        assert_eq!(
            driver.state,
            PresenceState::Error("Discord IPC unavailable")
        );
        let retry = driver.retry_at;
        driver.transport.fail_connect = false;
        driver.tick(retry - Duration::from_secs(1)).await;
        assert_eq!(driver.transport.connections, 1);
        driver.tick(retry).await;
        assert_eq!(driver.transport.connections, 2);
        assert_eq!(updates(&driver).len(), 1);
    }
    #[tokio::test]
    async fn disconnect_resends_latest_activity_even_when_unchanged() {
        let mut driver = driver();
        driver.desired = Some(activity(42));
        driver.tick(driver.retry_at).await;
        driver.handle_frame(Err("Discord IPC disconnected")).await;
        driver.transport.replies.push_back(ready());
        driver.tick(driver.retry_at).await;
        assert_eq!(updates(&driver).len(), 2);
        assert_eq!(driver.transport.connections, 2);
        assert_eq!(driver.transport.disconnects, 1);
    }
    #[tokio::test]
    async fn rejected_handshake_never_reports_connected_or_echoes_remote_data() {
        let mut driver = driver();
        driver.transport.replies = VecDeque::from([Frame {
            opcode: 2,
            body: json!({"message":"fake-private-body"}),
        }]);
        driver.tick(driver.retry_at).await;
        assert_eq!(
            driver.state,
            PresenceState::Error("Discord IPC handshake rejected")
        );
        assert!(updates(&driver).is_empty());
    }
    #[tokio::test]
    async fn ping_pong_and_remote_error() {
        let mut driver = driver();
        driver.tick(driver.retry_at).await;
        driver
            .handle_frame(Ok(Frame {
                opcode: 3,
                body: json!({"fake":1}),
            }))
            .await;
        assert_eq!(
            driver.transport.writes.last().unwrap(),
            &Frame {
                opcode: 4,
                body: json!({"fake":1})
            }
        );
        driver
            .handle_frame(Ok(Frame {
                opcode: 1,
                body: json!({"evt":"ERROR","data":{"message":"fake-private-body"}}),
            }))
            .await;
        assert_eq!(
            driver.state,
            PresenceState::Error("Discord IPC request rejected")
        );
    }
    #[tokio::test]
    async fn write_failure_retries_latest_value() {
        let mut driver = driver();
        driver.desired = Some(activity(1));
        driver.tick(driver.retry_at).await;
        driver.transport.fail_write = true;
        driver.desired = Some(activity(2));
        driver.tick(driver.last_send.unwrap() + INTERVAL).await;
        assert_eq!(
            driver.state,
            PresenceState::Error("Discord IPC write failed")
        );
        driver.transport.fail_write = false;
        driver.transport.replies.push_back(ready());
        driver.tick(driver.retry_at).await;
        assert_eq!(
            updates(&driver).last().unwrap()["args"]["activity"]["state"],
            "Test Printer · %2"
        );
    }
    #[tokio::test]
    async fn receive_preserves_partial_frames_across_cancellation() {
        let (stream, mut peer) = tokio::io::duplex(1024);
        let mut transport = IpcTransport {
            stream: Some(Box::new(stream)),
            incoming: Vec::new(),
        };
        let bytes = encode_frame(1, &json!({"evt":"READY"})).unwrap();
        peer.write_all(&bytes[..5]).await.unwrap();
        assert!(timeout(Duration::from_millis(10), transport.receive())
            .await
            .is_err());
        assert_eq!(transport.incoming.len(), 5);
        peer.write_all(&bytes[5..]).await.unwrap();
        assert_eq!(transport.receive().await.unwrap(), ready_without_command());
        drop(peer);
        assert_eq!(transport.receive().await, Err("Discord IPC disconnected"));
    }
    fn ready_without_command() -> Frame {
        Frame {
            opcode: 1,
            body: json!({"evt":"READY"}),
        }
    }
    #[test]
    fn all_unix_endpoint_variants_and_indices_are_scanned() {
        let paths = unix_paths([
            PathBuf::from("/fake/run"),
            PathBuf::from("/fake/tmp"),
            PathBuf::from("/tmp"),
        ]);
        for root in ["/fake/run", "/fake/tmp", "/tmp"] {
            for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
                for index in 0..10 {
                    assert!(paths.contains(
                        &PathBuf::from(root)
                            .join(sub)
                            .join(format!("discord-ipc-{index}"))
                    ));
                }
            }
        }
        assert_eq!(paths.len(), 90);
        assert_eq!(
            unix_paths([PathBuf::from("/tmp"), PathBuf::from("/tmp")]).len(),
            30
        );
    }
    #[test]
    fn invalid_id_and_missing_runtime_are_safe_errors() {
        assert!(matches!(
            PresenceClient::new("bad"),
            Err(PresenceState::Error("invalid application id"))
        ));
        assert!(matches!(
            PresenceClient::new("123456789012345678"),
            Err(PresenceState::Error("async runtime unavailable"))
        ));
    }

    #[tokio::test]
    async fn worker_flushes_pending_update_without_another_set() {
        let mut driver = driver();
        driver.state = PresenceState::Connected;
        driver.sent = Some(Some(activity(1)));
        driver.last_send = Some(Instant::now() - INTERVAL + Duration::from_millis(30));
        driver.transport.replies.clear();
        let (observed, mut writes) = tokio::sync::mpsc::unbounded_channel();
        driver.transport.observed = Some(observed);
        let (desired, changes) = watch::channel(Some(activity(2)));
        let (states, mut events) = watch::channel(PresenceState::Disconnected);
        let task = tokio::spawn(run(driver, changes, states));
        timeout(Duration::from_secs(2), events.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*events.borrow_and_update(), PresenceState::Connected);
        let update = timeout(Duration::from_secs(2), writes.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            update.body["args"]["activity"]["state"],
            "Test Printer · %2"
        );
        drop(desired);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*events.borrow(), PresenceState::Disconnected);
        assert!(writes.recv().await.is_none());
    }

    #[tokio::test]
    async fn worker_reconnects_without_another_set() {
        let mut driver = driver();
        driver.state = PresenceState::Error("Discord IPC unavailable");
        driver.retry_at = Instant::now() + Duration::from_millis(20);
        let (observed, mut writes) = tokio::sync::mpsc::unbounded_channel();
        driver.transport.observed = Some(observed);
        let (desired, changes) = watch::channel(Some(activity(42)));
        let (states, _) = watch::channel(PresenceState::Disconnected);
        let task = tokio::spawn(run(driver, changes, states));
        let handshake = timeout(Duration::from_secs(2), writes.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(handshake.opcode, 0);
        let update = timeout(Duration::from_secs(2), writes.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            update.body["args"]["activity"]["state"],
            "Test Printer · %42"
        );
        drop(desired);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }
}
