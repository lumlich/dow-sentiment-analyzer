use dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider;
use dow_sentiment_analyzer::ingest::types::SourceProvider;

// Use a 'static fixture via include_str! to cover the from_fixture(&'static str) path.
const FED_XML: &str = include_str!("fixtures/fed_rss.xml");

#[tokio::test]
async fn fed_fixture_static_parses_and_yields_events() {
    let provider = FedRssProvider::from_fixture(FED_XML);

    let items = provider.fetch_latest().await.expect("fed parse ok");
    assert!(
        !items.is_empty(),
        "Fed provider should produce at least one item from fixture"
    );
    assert!(
        items.iter().all(|e| !e.text.is_empty()),
        "Every item should have non-empty text"
    );
    assert!(
        items.iter().any(|e| e.source == "Fed"),
        "At least one item should have source = Fed"
    );
}
