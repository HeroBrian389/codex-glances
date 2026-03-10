use crate::data::DataCollector;
use crate::types::SessionRow;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub(super) fn spawn_refresh_worker(
    mut collector: DataCollector,
) -> (Sender<()>, Receiver<Result<Vec<SessionRow>, String>>) {
    let (request_tx, request_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<SessionRow>, String>>();

    thread::spawn(move || {
        while request_rx.recv().is_ok() {
            let result = collector.collect().map_err(|err| format!("{err:#}"));
            if result_tx.send(result).is_err() {
                break;
            }
        }
    });

    (request_tx, result_rx)
}
