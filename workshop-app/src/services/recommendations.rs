use std::time::Duration;

const BACKEND_LATENCY: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub(crate) struct Recommendations;

impl Recommendations {
    /// Models a request to an external recommendations service.
    pub(crate) async fn fetch(&self, product_id: u64) -> Vec<u64> {
        tokio::time::sleep(BACKEND_LATENCY).await;
        vec![product_id.saturating_add(1), product_id.saturating_add(2)]
    }
}
