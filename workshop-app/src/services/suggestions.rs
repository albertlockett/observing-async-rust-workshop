use std::time::Duration;

use dial9_utils::{dial9_span, span::Instrument};

const BACKEND_LATENCY: Duration = Duration::from_millis(80);

#[derive(Clone)]
pub(crate) struct Suggestions;

impl Suggestions {
    /// Models fetching from a remote suggestions service
    pub(crate) async fn fetch(&self, query: &str) -> Vec<String> {
        async {
            // stands for something like http_client.get(suggestions_url).send().await do not modify
            tokio::time::sleep(BACKEND_LATENCY).await;
            vec![format!("{query} gifts"), format!("{query} deals")]
        }
        .instrument(dial9_span!(
            "suggestions.fetch",
            query_len: u64 = query.len() as u64,
        ))
        .await
    }
}
