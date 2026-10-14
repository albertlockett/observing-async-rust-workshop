mod catalog;
mod checkout;
mod events;
mod search;

use axum::Json;
use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use dial9_utils::dial9_span;
use dial9_utils::tower::Dial9SpanLayerWithResponse;
use serde::Serialize;

use crate::model::ServiceInfo;
use crate::state::AppState;

pub(crate) fn router(state: AppState) -> Router {
    let request_spans = Dial9SpanLayerWithResponse::new(|request: &Request| {
        let route = request
            .extensions()
            .get::<MatchedPath>()
            .map(|path| path.as_str().to_owned())
            .unwrap_or_else(|| "unmatched".to_owned());
        let (span, slots) = dial9_span!(
            "http.request",
            method = %request.method(),
            route: String = route,
            status: u64,
        );
        (span, move |response: &Response| {
            slots.status.set(u64::from(response.status().as_u16()))
        })
    });

    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/products/{id}", get(catalog::product))
        .route("/events", post(events::ingest))
        .route("/search", get(search::search))
        .route("/checkout", post(checkout::checkout))
        .with_state(state)
        .layer(request_spans)
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
