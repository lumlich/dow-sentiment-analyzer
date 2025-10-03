// src/ingest/providers/mod.rs

pub mod fed_rss;
pub mod reuters_rss;
pub mod generic_rss;

// Re-export trait and event type so provider modules can use `super::{Provider, ProviderItem};`
pub use crate::ingest::types::{SourceEvent, SourceProvider as Provider};

/// Local alias to match provider code that expects `ProviderItem`.
pub type ProviderItem = SourceEvent;
