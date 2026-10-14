mod catalog;
mod checkout;
mod events;
mod search;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Serialize;

use crate::model::ServiceInfo;
use crate::state::AppState;

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/products/{id}", get(catalog::product))
        .route("/events", post(events::ingest))
        .route("/search", get(search::search))
        .route("/checkout", post(checkout::checkout))
        .with_state(state)
}

async fn index() -> Json<ServiceInfo> {
    Json(ServiceInfo {
        service: "workshop-app",
        routes: &[
            "GET /products/{id}",
            "POST /events",
            "GET /search?q=...",
            "POST /checkout",
        ],
    })
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub(super) enum ApiError {
    BadRequest(&'static str),
    NotFound(&'static str),
    Conflict(&'static str),
    Unavailable(&'static str),
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Self::BadRequest(error) => (StatusCode::BAD_REQUEST, error),
            Self::NotFound(error) => (StatusCode::NOT_FOUND, error),
            Self::Conflict(error) => (StatusCode::CONFLICT, error),
            Self::Unavailable(error) => (StatusCode::SERVICE_UNAVAILABLE, error),
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}
