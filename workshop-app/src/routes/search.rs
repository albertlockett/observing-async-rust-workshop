use axum::Json;
use axum::extract::{Query, State};
use regex::Regex;
use serde::Deserialize;

use crate::model::{ProductSummary, SearchResponse};
use crate::state::AppState;

use super::ApiError;

const UNICODE_SEARCH_QUERY_PATTERN: &str = r"(?iu)^[\p{L}\p{N}][\p{L}\p{N}\s.,'_-]{0,63}$";

#[derive(Deserialize)]
pub(super) struct SearchParams {
    q: String,
    limit: Option<usize>,
    suggestions: Option<bool>,
}

pub(super) async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    let query = params.q.trim();
    if !valid_search_query(query) {
        return Err(ApiError::BadRequest("q has an invalid format"));
    }
    let limit = params.limit.unwrap_or(10).clamp(1, 50);
    let products = state
        .catalog
        .search(query, limit)
        .into_iter()
        .map(ProductSummary::from)
        .collect();
    // Suggestions enrich search results for clients that request them.
    let suggestions = if params.suggestions.unwrap_or(false) {
        state.suggestions.fetch(query).await
    } else {
        Vec::new()
    };

    Ok(Json(SearchResponse {
        query: query.to_string(),
        products,
        suggestions,
    }))
}

#[inline(never)]
fn valid_search_query(query: &str) -> bool {
    if query.is_ascii() {
        let Some(first) = query.bytes().next() else {
            return false;
        };
        return query.len() <= 64
            && first.is_ascii_alphanumeric()
            && query.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b' ' | b'.' | b',' | b'\'' | b'_' | b'-')
            });
    }

    Regex::new(UNICODE_SEARCH_QUERY_PATTERN)
        .expect("search query pattern is valid")
        .is_match(query)
}
