use dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider;
use dow_sentiment_analyzer::ingest::types::SourceProvider;
use std::fs;

#[tokio::test]
async fn reuters_fixture_string_parses_and_yields_events() {
    // Load XML fixture as String
    let xml = fs::read_to_string("tests/fixtures/reuters_rss.xml")
        .expect("missing tests/fixtures/reuters_rss.xml");

    // Use the non-'static constructor to avoid lifetime issues
    let provider = ReutersRssProvider::from_fixture_str(&xml);

    let items = provider.fetch_latest().await.expect("reuters parse ok");
    assert!(
        !items.is_empty(),
        "Reuters provider should produce at least one item from fixture"
    );
    assert!(
        items.iter().all(|e| !e.text.is_empty()),
        "Every item should have non-empty text"
    );
    assert!(
        items.iter().any(|e| e.source == "Reuters"),
        "At least one item should have source = Reuters"
    );
}
