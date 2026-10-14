use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::model::Product;

const RESERVATION_WRITE_LATENCY: Duration = Duration::from_millis(40);

#[derive(Debug)]
pub(crate) enum ReserveError {
    UnknownProduct,
    InsufficientStock,
}

#[derive(Clone)]
pub(crate) struct Inventory {
    // Maps product ID to available quantity
    available: Arc<Mutex<HashMap<u64, u32>>>,
}

impl Inventory {
    pub(crate) fn new(products: &[Product]) -> Self {
        let available = products
            .iter()
            .map(|product| (product.id, product.stock))
            .collect();
        Self {
            available: Arc::new(Mutex::new(available)),
        }
    }

    /// Models a request to an external inventory service.
    pub(crate) async fn available(&self, product_id: u64) -> Option<u32> {
        tokio::time::sleep(Duration::from_millis(2)).await;
        self.available.lock().await.get(&product_id).copied()
    }

    /// Models an inventory reservation and its durable write.
    pub(crate) async fn reserve(&self, product_id: u64, quantity: u32) -> Result<(), ReserveError> {
        let mut available = self.available.lock().await;
        let stock = available
            .get_mut(&product_id)
            .ok_or(ReserveError::UnknownProduct)?;
        if *stock < quantity {
            return Err(ReserveError::InsufficientStock);
        }
        *stock -= quantity;

        // Keep the in-memory update and durable write together so another
        // reservation cannot observe state that has not been persisted.
        persist_reservation(product_id, quantity).await;
        Ok(())
    }
}

async fn persist_reservation(product_id: u64, quantity: u32) {
    tokio::time::sleep(RESERVATION_WRITE_LATENCY).await;
    std::hint::black_box((product_id, quantity));
}
