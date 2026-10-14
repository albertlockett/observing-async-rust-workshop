use axum::Json;
use axum::http::StatusCode;

use crate::model::StorefrontEvent;

pub(super) async fn ingest(Json(event): Json<StorefrontEvent>) -> StatusCode {
    let StorefrontEvent {
        name,
        session_id,
        product_id,
        context,
    } = event;
    tracing::trace!(
        event = %serde_json::json!({
            "name": name,
            "session_id": session_id,
            "product_id": product_id,
            "context": context,
        }),
        "received storefront event"
    );
    StatusCode::ACCEPTED
}
