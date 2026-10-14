use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::model::AuditEvent;
use crate::services::{Catalog, Inventory, Pricing, Recommendations, Suggestions};

#[derive(Clone)]
pub(crate) struct AppState {
    pub catalog: Catalog,
    pub pricing: Pricing,
    pub inventory: Inventory,
    pub recommendations: Recommendations,
    pub suggestions: Suggestions,
    audit: mpsc::Sender<AuditEvent>,
    next_order_id: Arc<AtomicU64>,
}

impl AppState {
    pub(crate) fn new(
        catalog: Catalog,
        inventory: Inventory,
        audit: mpsc::Sender<AuditEvent>,
    ) -> Self {
        Self {
            catalog,
            pricing: Pricing,
            inventory,
            recommendations: Recommendations,
            suggestions: Suggestions,
            audit,
            next_order_id: Arc::new(AtomicU64::new(10_000)),
        }
    }

    pub(crate) fn next_order_id(&self) -> u64 {
        self.next_order_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) async fn record(&self, event: AuditEvent) -> Result<(), AuditEvent> {
        self.audit.send(event).await.map_err(|error| error.0)
    }
}
