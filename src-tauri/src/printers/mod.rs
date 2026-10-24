//! Printer adapters. Each one reads telemetry over the local network and never sends
//! control commands to the printer.
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::model::SourceEvent;

pub mod bambu;

#[async_trait]
pub trait PrinterSource: Send + 'static {
    /// Runs until `shutdown` is cancelled, reconnecting on its own, and reports through
    /// `events`. Must never log credentials, serial numbers or addresses.
    async fn run(self: Box<Self>, events: mpsc::Sender<SourceEvent>, shutdown: CancellationToken);
}
