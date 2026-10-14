use std::time::Duration;

use tokio::sync::mpsc;

use crate::model::AuditEvent;

pub(super) async fn run(mut events: mpsc::Receiver<AuditEvent>) {
    while let Some(event) = events.recv().await {
        // Encoding and a short delay stand in for a remote audit sink. Keeping
        // it behind a bounded channel prevents it from delaying checkout.
        if serde_json::to_vec(&event).is_err() {
            eprintln!("failed to encode audit event");
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
