use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::model::{AuditEvent, CheckoutRequest, CheckoutResponse};
use crate::services::ReserveError;
use crate::state::AppState;

use super::ApiError;

pub(super) async fn checkout(
    State(state): State<AppState>,
    Json(request): Json<CheckoutRequest>,
) -> Result<(StatusCode, Json<CheckoutResponse>), ApiError> {
    if !(1..=10).contains(&request.quantity) {
        return Err(ApiError::BadRequest("quantity must be between 1 and 10"));
    }
    let product = state
        .catalog
        .get(request.product_id)
        .ok_or(ApiError::NotFound("product not found"))?;
    let unit_price = state.pricing.quote(&product).await;
    state
        .inventory
        .reserve(request.product_id, request.quantity)
        .await
        .map_err(|error| match error {
            ReserveError::UnknownProduct => ApiError::Unavailable("inventory unavailable"),
            ReserveError::InsufficientStock => ApiError::Conflict("insufficient stock"),
        })?;

    let total_cents = unit_price
        .checked_mul(u64::from(request.quantity))
        .ok_or(ApiError::BadRequest("order total is too large"))?;
    let order_id = state.next_order_id();
    state
        .record(AuditEvent {
            order_id,
            product_id: request.product_id,
            quantity: request.quantity,
            total_cents,
        })
        .await
        .map_err(|_| ApiError::Unavailable("audit queue unavailable"))?;

    Ok((
        StatusCode::CREATED,
        Json(CheckoutResponse {
            order_id,
            product_id: request.product_id,
            quantity: request.quantity,
            total_cents,
        }),
    ))
}
