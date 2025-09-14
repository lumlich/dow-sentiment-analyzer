// src/ingest/scheduler.rs
use crate::ingest::{
    providers::{fed_rss::FedRssProvider, reuters_rss::ReutersRssProvider},
    types::SourceProvider,
};
use metrics::{counter, gauge};
use tokio::task::JoinHandle;

#[derive(Clone, Copy, Debug)]
pub struct IngestSchedulerCfg {
    pub interval_secs: u64,
    pub dedup_window_secs: u64,
}

/// Spawn a lightweight scheduler that ingests from embedded fixtures.
/// Requires feature `ingest-fixtures`.
#[cfg(feature = "ingest-fixtures")]
pub fn spawn_fixture_scheduler(cfg: IngestSchedulerCfg, whitelist: Vec<String>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(cfg.interval_secs));
        loop {
            ticker.tick().await;
            let now = chrono::Utc::now().timestamp().max(0) as u64;

            // Build providers from embedded fixtures
            let fed_xml: &str = include_str!("../../tests/fixtures/fed_rss.xml");
            let reu_xml: &str = include_str!("../../tests/fixtures/reuters_rss.xml");

            let providers: Vec<Box<dyn SourceProvider>> = vec![
                Box::new(FedRssProvider::from_fixture(fed_xml)),
                Box::new(ReutersRssProvider::from_fixture(reu_xml)),
            ];

            let (kept, filtered, dedup) =
                crate::ingest::run_once(&providers, &whitelist, cfg.dedup_window_secs).await;

            counter!("ingest_runs_total").increment(1);
            gauge!("ingest_pipeline_last_run_ts").set(now as f64);

            tracing::info!(
                target: "ingest",
                kept = kept.len(),
                filtered = filtered,
                dedup = dedup,
                "fixture ingest tick"
            );
        }
    })
}

#[cfg(not(feature = "ingest-fixtures"))]
pub fn spawn_fixture_scheduler(
    _cfg: IngestSchedulerCfg,
    _whitelist: Vec<String>,
) -> JoinHandle<()> {
    panic!("spawn_fixture_scheduler called without feature `ingest-fixtures`");
}
