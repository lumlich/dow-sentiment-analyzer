// tests/backup_cron.rs
use dow_sentiment_analyzer::ingest::backup::{backup_configs_once, MockSink};

#[tokio::test]
async fn backup_sink_is_called_with_config_files() {
    // Ensure the sample whitelist exists (repo ships config/authority_whitelist.json).
    let sink = MockSink::new();
    backup_configs_once(&sink).await.expect("ok");

    let calls = sink.calls.lock().unwrap();
    assert!(!calls.is_empty());
    let first = &calls[0];
    // Expect at least one JSON config in payload
    assert!(first.iter().any(|(path, _content)| path.ends_with(".json")));
}
