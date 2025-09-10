// tests/providers_reuters.rs
use dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider;
use dow_sentiment_analyzer::ingest::types::SourceProvider;
use std::fs;

#[tokio::test]
async fn parses_reuters_fixture() {
    let xml = fs::read_to_string("tests/fixtures/reuters_rss.xml").expect("fixture");
    let p = ReutersRssProvider::from_fixture(&xml);
    let evs = p.fetch_latest().await.expect("ok");

    assert_eq!(evs.len(), 2);
    assert!(evs.iter().all(|e| e.source == "Reuters"));
    assert!(evs.iter().all(|e| e.published_at > 0));
    assert!(evs.iter().all(|e| e.url.is_some()));
}
