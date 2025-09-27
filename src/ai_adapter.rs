//! Back-compat shim so tests and older code can use `dow_sentiment_analyzer::ai_adapter`.
//! We re-export everything from `analyze::ai_adapter`.

pub use crate::analyze::ai_adapter::*;
