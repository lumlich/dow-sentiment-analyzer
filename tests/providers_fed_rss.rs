// tests/providers_fed_rss.rs
use std::fs;

use dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider;
use dow_sentiment_analyzer::ingest::types::{SourceEvent, SourceProvider};

#[tokio::test]
async fn parses_fixture_into_source_events() {
    let xml = fs::read_to_string("tests/fixtures/fed_rss.xml").expect("fixture");
    let p = FedRssProvider::from_fixture(&xml);

    let events = p.fetch_latest().await.expect("parsed");
    assert!(!events.is_empty());

    let first: &SourceEvent = &events[0];
    assert_eq!(first.source, "Fed");
    assert!(first.text.len() > 5);
    assert!(first.published_at > 0);
    assert!(first.url.as_ref().unwrap().starts_with("https://"));
    assert!(first.priority_hint.unwrap() > 0.0);
}
