// src/ingest/types.rs
use anyhow::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SourceEvent {
    pub source: String,    // e.g., "Fed", "Reuters"
    pub published_at: u64, // unix seconds
    pub text: String,      // normalized text
    pub url: Option<String>,
    pub priority_hint: Option<i32>,
}

#[async_trait::async_trait]
pub trait SourceProvider {
    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>>;
    fn name(&self) -> &'static str;
}
