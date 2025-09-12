// src/ingest/types.rs
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceEvent {
    /// Logical source name, e.g., "Fed", "Reuters".
    pub source: String,
    /// Unix timestamp (seconds) when the content was published.
    pub published_at: u64,
    /// Human-readable text after normalization. Must be non-empty to be useful.
    pub text: String,
    /// Optional permalink to the content.
    pub url: Option<String>,
    /// Optional importance hint ~[0.0, 1.0]; higher means "pay attention".
    pub priority_hint: Option<f32>,
}

#[async_trait::async_trait]
pub trait SourceProvider: Send + Sync {
    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>>;
    fn name(&self) -> &'static str;
}
