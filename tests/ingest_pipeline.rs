// tests/ingest_pipeline.rs
use anyhow::Result;
use async_trait::async_trait;
use dow_sentiment_analyzer::ingest::types::{SourceEvent, SourceProvider};

struct MockProvider;

#[async_trait]
impl SourceProvider for MockProvider {
    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>> {
        Ok(vec![SourceEvent {
            source: "Fed".to_string(),
            text: "<b>Hello&nbsp;world</b> &ldquo;ok&rdquo;".to_string(),
            published_at: 1_000_000,
            url: Some("https://example.test/x".to_string()),
            priority_hint: Some(0.8),
        }])
    }
    fn name(&self) -> &'static str {
        "MockProvider"
    }
}

#[tokio::test]
async fn smoke_pipeline_runs_and_outputs() {
    let providers: Vec<Box<dyn SourceProvider>> = vec![Box::new(MockProvider)];
    let out = dow_sentiment_analyzer::ingest::run_once_with_empty_whitelist(&providers).await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, r#"Hello world "ok""#);
}
