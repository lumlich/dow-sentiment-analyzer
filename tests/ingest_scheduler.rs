// tests/ingest_scheduler.rs
use dow_sentiment_analyzer::ingest::providers::{
    fed_rss::FedRssProvider, reuters_rss::ReutersRssProvider,
};
use dow_sentiment_analyzer::ingest::{self, types::SourceProvider};

#[tokio::test]
async fn smoke_run_once_with_fixtures_keeps_some() {
    let fed_xml: &str = include_str!("fixtures/fed_rss.xml");
    let reu_xml: &str = include_str!("fixtures/reuters_rss.xml");
    let providers: Vec<Box<dyn SourceProvider>> = vec![
        Box::new(FedRssProvider::from_fixture(fed_xml)),
        Box::new(ReutersRssProvider::from_fixture(reu_xml)),
    ];
    let (kept, _filtered, _dedup) =
        ingest::run_once(&providers, &vec!["Fed".into(), "Reuters".into()], 600).await;
    assert!(!kept.is_empty());
}
