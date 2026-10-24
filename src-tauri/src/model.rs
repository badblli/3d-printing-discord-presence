//! Printer-agnostic types shared by adapters, presence and the UI.
//! Adapters translate their protocol into these; nothing downstream knows the brand.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterKind {
    Bambu,
    Moonraker,
    OctoPrint,
    PrusaLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintState {
    Idle,
    Preparing,
    Printing,
    Paused,
    Finished,
    Failed,
}

impl PrintState {
    /// States that should be shown on the Discord card.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Preparing | Self::Printing | Self::Paused)
    }
}

/// One normalized observation of a printer. Optional fields are `None` when the printer
/// did not report them or reported an out-of-range value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintStatus {
    pub kind: PrinterKind,
    pub state: PrintState,
    /// 0..=100
    pub progress: Option<u8>,
    /// Seconds left as reported at `observed_at_ms`.
    pub remaining_secs: Option<u32>,
    pub file_name: Option<String>,
    pub printer_model: Option<String>,
    pub layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub nozzle_temp: Option<f32>,
    pub bed_temp: Option<f32>,
    /// Unix time in milliseconds when this status was observed.
    pub observed_at_ms: u64,
}

/// Machine-readable reasons; the UI turns them into localized text and fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionError {
    Unreachable,
    AuthRejected,
    SubscribeDenied,
    Stale,
    Protocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Error(ConnectionError),
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceEvent {
    Connection { state: ConnectionState },
    /// `None` clears the last status (no data, stale data or no job).
    Status { status: Option<PrintStatus> },
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
