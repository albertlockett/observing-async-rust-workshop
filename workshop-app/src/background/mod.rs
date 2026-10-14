mod audit;
mod cache_refresh;

use tokio::sync::mpsc;

use crate::model::AuditEvent;
use crate::services::Catalog;

pub(crate) fn start(catalog: Catalog, audit_events: mpsc::Receiver<AuditEvent>) {
    let handle = dial9::Dial9TokioHandle::current();
    handle.spawn(cache_refresh::run(catalog));
    handle.spawn(audit::run(audit_events));
}
