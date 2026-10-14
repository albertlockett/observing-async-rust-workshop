use std::time::Duration;

use crate::services::Catalog;

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

pub(super) async fn run(catalog: Catalog) {
    let mut interval = tokio::time::interval(REFRESH_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Consume the immediate first tick, startup already loaded the catalog.
    interval.tick().await;

    loop {
        interval.tick().await;
        if let Err(error) = catalog.refresh().await {
            eprintln!("catalog refresh failed: {error}");
        }
    }
}
