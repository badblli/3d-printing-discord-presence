//! Read-only Bambu LAN MQTT telemetry. No printer control operations.
pub mod telemetry;
pub mod tls;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use rumqttc::{
    AsyncClient, ConnectReturnCode, Event, Incoming, MqttOptions, QoS, SubscribeReasonCode,
    Transport,
};
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};
use tokio_util::sync::CancellationToken;

use super::PrinterSource;
use crate::model::{now_ms, ConnectionError, ConnectionState, SourceEvent};
use telemetry::Telemetry;

const PUSHALL: &[u8] = br#"{"pushing":{"sequence_id":"0","command":"pushall"}}"#;
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);
const STALE_AFTER: Duration = Duration::from_secs(90);

// Deliberately no Debug: all three connection fields are private data.
pub struct BambuSource {
    pub host: String,
    pub serial: String,
    pub access_code: String,
    pub model: Option<String>,
}

fn valid_serial(serial: &str) -> bool {
    (8..=32).contains(&serial.len()) && serial.bytes().all(|c| c.is_ascii_alphanumeric())
}

/// The sole outbound publish path; callers cannot supply a topic, command or payload.
fn request_snapshot(client: &AsyncClient, serial: &str) -> Result<(), ConnectionError> {
    client
        .try_publish(
            format!("device/{serial}/request"),
            QoS::AtMostOnce,
            false,
            PUSHALL,
        )
        .map_err(|_| ConnectionError::Unreachable)
}

fn connection_error(error: &rumqttc::ConnectionError) -> ConnectionError {
    match error {
        rumqttc::ConnectionError::ConnectionRefused(
            ConnectReturnCode::BadUserNamePassword | ConnectReturnCode::NotAuthorized,
        ) => ConnectionError::AuthRejected,
        // A rejected certificate or handshake is not a network problem; keep it apart
        // so firmware incompatibilities are visible.
        rumqttc::ConnectionError::Tls(_) => ConnectionError::Protocol,
        _ => ConnectionError::Unreachable,
    }
}

fn next_backoff(delay: Duration) -> Duration {
    (delay * 2).min(Duration::from_secs(30))
}

async fn emit(
    events: &mpsc::Sender<SourceEvent>,
    shutdown: &CancellationToken,
    event: SourceEvent,
) -> bool {
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => false,
        result = events.send(event) => result.is_ok(),
    }
}

async fn connection(
    events: &mpsc::Sender<SourceEvent>,
    shutdown: &CancellationToken,
    state: ConnectionState,
) -> bool {
    emit(events, shutdown, SourceEvent::Connection { state }).await
}

impl BambuSource {
    /// A fresh MQTT session per attempt avoids replaying queued requests on reconnect.
    async fn session(
        &self,
        config: Arc<rumqttc::tokio_rustls::rustls::ClientConfig>,
        events: &mpsc::Sender<SourceEvent>,
        shutdown: &CancellationToken,
    ) -> Result<(), ConnectionError> {
        let mut options = MqttOptions::new(
            format!("printpresence-{}", uuid::Uuid::new_v4()),
            &self.host,
            8883,
        );
        options.set_credentials("bblp", &self.access_code);
        options.set_transport(Transport::tls_with_config(
            rumqttc::TlsConfiguration::Rustls(config),
        ));
        options.set_keep_alive(Duration::from_secs(30));
        options.set_max_packet_size(telemetry::MAX_PAYLOAD + 1024, 1024);
        let (client, mut eventloop) = AsyncClient::new(options, 8);
        eventloop.network_options.set_connection_timeout(10);
        let report_topic = format!("device/{}/report", self.serial);
        let mut telemetry = Telemetry::default();
        let mut subscribed = false;
        let mut stale = false;
        let mut last_report = Instant::now();
        let mut next_snapshot = Instant::now() + SNAPSHOT_INTERVAL;
        let mut tick = time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                _ = events.closed() => return Ok(()),
                event = eventloop.poll() => match event {
                    Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                        client.try_subscribe(&report_topic, QoS::AtMostOnce)
                            .map_err(|_| ConnectionError::SubscribeDenied)?;
                    }
                    Ok(Event::Incoming(Incoming::SubAck(ack))) if !subscribed => {
                        if ack.return_codes.len() != 1 || ack.return_codes.iter().any(|code| matches!(code, SubscribeReasonCode::Failure)) {
                            return Err(ConnectionError::SubscribeDenied);
                        }
                        subscribed = true;
                        last_report = Instant::now();
                        request_snapshot(&client, &self.serial)?;
                        next_snapshot = Instant::now() + SNAPSHOT_INTERVAL;
                        if !connection(events, shutdown, ConnectionState::Connected).await { return Ok(()); }
                    }
                    Ok(Event::Incoming(Incoming::Publish(packet))) if subscribed && packet.topic == report_topic => {
                        let now = Instant::now();
                        if telemetry.ingest(&packet.payload, now.into_std()) {
                            last_report = now;
                            if stale {
                                stale = false;
                                if !connection(events, shutdown, ConnectionState::Connected).await { return Ok(()); }
                            }
                            let status = telemetry.status(now.into_std(), now_ms(), self.model.as_deref());
                            if !emit(events, shutdown, SourceEvent::Status { status }).await { return Ok(()); }
                        }
                    }
                    Ok(Event::Incoming(Incoming::Disconnect)) => return Err(ConnectionError::Unreachable),
                    Err(error) => return Err(connection_error(&error)),
                    _ => {}
                },
                // Do not cancel an in-progress DNS/TCP/TLS connection every second.
                _ = tick.tick(), if subscribed => {
                    let now = Instant::now();
                    if subscribed && now >= next_snapshot {
                        request_snapshot(&client, &self.serial)?;
                        next_snapshot = now + SNAPSHOT_INTERVAL;
                    }
                    if subscribed && !stale && now.duration_since(last_report) > STALE_AFTER {
                        stale = true;
                        if !emit(events, shutdown, SourceEvent::Status { status: None }).await
                            || !connection(events, shutdown, ConnectionState::Error(ConnectionError::Stale)).await { return Ok(()); }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl PrinterSource for BambuSource {
    async fn run(self: Box<Self>, events: mpsc::Sender<SourceEvent>, shutdown: CancellationToken) {
        let config = if valid_serial(&self.serial) {
            tls::client_config(&self.serial).ok().map(Arc::new)
        } else {
            None
        };
        if let Some(config) = config {
            let mut delay = Duration::from_secs(2);
            loop {
                if !connection(&events, &shutdown, ConnectionState::Connecting).await {
                    break;
                }
                let started = Instant::now();
                match self.session(config.clone(), &events, &shutdown).await {
                    Ok(()) => break,
                    Err(error) => {
                        if !emit(&events, &shutdown, SourceEvent::Status { status: None }).await
                            || !connection(&events, &shutdown, ConnectionState::Error(error)).await
                        {
                            break;
                        }
                    }
                }
                if started.elapsed() >= SNAPSHOT_INTERVAL {
                    delay = Duration::from_secs(2);
                }
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = events.closed() => break,
                    _ = time::sleep(delay) => {}
                }
                delay = next_backoff(delay);
            }
        } else {
            connection(
                &events,
                &shutdown,
                ConnectionState::Error(ConnectionError::Protocol),
            )
            .await;
        }
        // Never block shutdown behind a full or abandoned event channel.
        let _ = events.try_send(SourceEvent::Status { status: None });
        let _ = events.try_send(SourceEvent::Connection {
            state: ConnectionState::Stopped,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bambu_only_publishes_pushall() {
        let options = MqttOptions::new("fake-client", "192.168.1.50", 8883);
        let (client, mut eventloop) = AsyncClient::new(options, 4);
        request_snapshot(&client, "00M00A000000000").unwrap();
        eventloop.clean();
        assert_eq!(eventloop.pending.len(), 1);
        match eventloop.pending.pop_front().unwrap() {
            rumqttc::Request::Publish(packet) => {
                assert_eq!(packet.topic, "device/00M00A000000000/request");
                assert_eq!(packet.payload.as_ref(), PUSHALL);
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&packet.payload).unwrap(),
                    serde_json::json!({"pushing":{"sequence_id":"0","command":"pushall"}})
                );
                assert!(!packet.retain);
                assert_eq!(packet.qos, QoS::AtMostOnce);
            }
            _ => panic!("unexpected outbound request"),
        }
    }

    #[test]
    fn bambu_backoff_caps_at_thirty_seconds() {
        let mut delay = Duration::from_secs(2);
        for expected in [4, 8, 16, 30, 30] {
            delay = next_backoff(delay);
            assert_eq!(delay.as_secs(), expected);
        }
    }

    #[test]
    fn bambu_auth_errors_are_codes() {
        for code in [
            ConnectReturnCode::BadUserNamePassword,
            ConnectReturnCode::NotAuthorized,
        ] {
            assert_eq!(
                connection_error(&rumqttc::ConnectionError::ConnectionRefused(code)),
                ConnectionError::AuthRejected
            );
        }
        assert_eq!(
            connection_error(&rumqttc::ConnectionError::NetworkTimeout),
            ConnectionError::Unreachable
        );
    }

    #[test]
    fn bambu_serial_cannot_inject_topics() {
        assert!(valid_serial("00M00A000000000"));
        for serial in ["", "short", "00M00A000000000/#", "00M00A000000000/other"] {
            assert!(!valid_serial(serial));
        }
    }

    #[tokio::test]
    async fn bambu_cancelled_source_stops_without_network() {
        let (tx, mut rx) = mpsc::channel(8);
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        Box::new(BambuSource {
            host: "192.168.1.50".into(),
            serial: "00M00A000000000".into(),
            access_code: "12345678".into(),
            model: None,
        })
        .run(tx, shutdown)
        .await;
        assert_eq!(rx.recv().await, Some(SourceEvent::Status { status: None }));
        assert_eq!(
            rx.recv().await,
            Some(SourceEvent::Connection {
                state: ConnectionState::Stopped
            })
        );
    }
}
