use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::model::ProductView;
use crate::state::AppState;

use super::ApiError;

const RECOMMENDATIONS_BUDGET: Duration = Duration::from_millis(25);

#[derive(Default, Deserialize)]
pub(super) struct ProductParams {
    #[serde(default)]
    recommendations: bool,
}

pub(super) async fn product(
    State(state): State<AppState>,
    Path(product_id): Path<u64>,
    Query(params): Query<ProductParams>,
) -> Result<Json<ProductView>, ApiError> {
    let product = state
        .catalog
        .get(product_id)
        .ok_or(ApiError::NotFound("product not found"))?;
    let price_cents = state.pricing.quote(&product).await;
    let available = state
        .inventory
        .available(product_id)
        .await
        .ok_or(ApiError::Unavailable("inventory unavailable"))?;
    let recommendations = if params.recommendations {
        let service = state.recommendations.clone();
        let task = dial9::Dial9TokioHandle::current()
            .spawn(async move { service.fetch(product_id).await });
        tokio::time::timeout(RECOMMENDATIONS_BUDGET, task)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let response = ProductView {
        id: product.id,
        name: product.name,
        category: product.category,
        price_cents,
        available,
        recommendations,
    };

    Ok(Json(response))
}
