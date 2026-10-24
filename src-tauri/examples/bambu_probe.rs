//! Two-minute LAN smoke test. Set BAMBU_HOST, BAMBU_SERIAL and BAMBU_CODE in your shell.
//! No credentials or print filenames are written to output. No logger is installed.
use std::{env, time::Duration};

use printpresence_lib::{
    model::SourceEvent,
    printers::{bambu::BambuSource, PrinterSource},
};
use tokio::{sync::mpsc, time};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let (Ok(host), Ok(serial), Ok(access_code)) = (
        env::var("BAMBU_HOST"),
        env::var("BAMBU_SERIAL"),
        env::var("BAMBU_CODE"),
    ) else {
        eprintln!("Set BAMBU_HOST, BAMBU_SERIAL and BAMBU_CODE before running bambu_probe.");
        return;
    };
    let source = BambuSource {
        host,
        serial,
        access_code,
        model: None,
    };
    let (events, mut receiver) = mpsc::channel(32);
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(Box::new(source).run(events, shutdown.clone()));
    let deadline = time::sleep(Duration::from_secs(120));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = receiver.recv() => match event {
                Some(SourceEvent::Status { status: Some(mut status) }) => {
                    // Personal filenames must never enter diagnostic output.
                    status.file_name = None;
                    println!("{status:?}");
                }
                Some(SourceEvent::Connection { state }) => println!("{state:?}"),
                Some(SourceEvent::Status { status: None }) => println!("Status cleared"),
                None => break,
            }
        }
    }
    shutdown.cancel();
    if task.await.is_err() {
        eprintln!("ProbeTaskFailed");
    }
}
