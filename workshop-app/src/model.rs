use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Product {
    pub id: u64,
    pub name: String,
    pub category: String,
    pub base_price_cents: u64,
    pub stock: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProductView {
    pub id: u64,
    pub name: String,
    pub category: String,
    pub price_cents: u64,
    pub available: u32,
    pub recommendations: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SearchResponse {
    pub query: String,
    pub products: Vec<ProductSummary>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProductSummary {
    pub id: u64,
    pub name: String,
    pub category: String,
    pub base_price_cents: u64,
}

impl From<Product> for ProductSummary {
    fn from(product: Product) -> Self {
        Self {
            id: product.id,
            name: product.name,
            category: product.category,
            base_price_cents: product.base_price_cents,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckoutRequest {
    pub product_id: u64,
    pub quantity: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckoutResponse {
    pub order_id: u64,
    pub product_id: u64,
    pub quantity: u32,
    pub total_cents: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServiceInfo {
    pub service: &'static str,
    pub routes: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub(crate) struct AuditEvent {
    pub order_id: u64,
    pub product_id: u64,
    pub quantity: u32,
    pub total_cents: u64,
}

#[derive(Deserialize)]
pub(crate) struct StorefrontEvent {
    pub name: String,
    pub session_id: String,
    pub product_id: u64,
    pub context: String,
}
