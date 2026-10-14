use std::time::Duration;

use crate::model::Product;

#[derive(Clone)]
pub(crate) struct Pricing;

impl Pricing {
    /// Models a small request to an external pricing service.
    pub(crate) async fn quote(&self, product: &Product) -> u64 {
        tokio::time::sleep(Duration::from_millis(3)).await;
        match product.category.as_str() {
            "books" => product.base_price_cents.saturating_mul(9) / 10,
            _ => product.base_price_cents,
        }
    }
}
