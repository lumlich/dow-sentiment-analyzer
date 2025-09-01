// src/analyze/mod.rs
//! Analysis pipeline entry: builds the decision result and enriches reasons via NER.

pub mod ai_adapter;
pub mod antispam;
pub mod debug;
pub mod ner;
pub mod rerank;
pub mod rules;
pub mod scoring;
pub mod weights;

use crate::analyze::ner::enrich_reasons;
use serde::Serialize;
use std::sync::OnceLock;
use std::time::SystemTime;

// Re-export convenient types.
pub use crate::analyze::antispam::{AntiSpam, AntiSpamParams};
pub use crate::analyze::rules::{HotReloadRules, RuleSet};
pub use crate::analyze::scoring::{base_confidence, ScoreInputs};
pub use crate::analyze::weights::{HotReloadWeights, Weights};

/// Global hot-reloaded configs.
static HOT_WEIGHTS: OnceLock<HotReloadWeights> = OnceLock::new();
static HOT_RULES: OnceLock<HotReloadRules> = OnceLock::new();

/// Final response returned by the /decide endpoint.
#[derive(Debug, Serialize)]
pub struct DecisionResult {
    pub action: String,
    pub confidence: f32,
    pub reasons: Vec<String>,
    // Add other fields as needed (e.g., score breakdowns, timing, etc.)
}

/// Backwards-compatible entry: calls the richer API with neutral signals.
pub fn analyze_and_decide(input_text: &str) -> DecisionResult {
    // Neutral default inputs: can be replaced by your upstream pipeline later.
    let inputs = ScoreInputs::new(0.5, 0.5, 0.5);
    analyze_and_decide_with_signals(input_text, inputs)
}

/// Main analysis function with explicit scoring inputs (Phase 3 integration).
/// Order:
/// 1) NER enrichment (config/*.json)
/// 2) Base confidence from calibrated weights (config/weights.json)
/// 3) Contextual rules (config/rules.json) that can set action / boost confidence / add reasons
pub fn analyze_and_decide_with_signals(input_text: &str, inputs: ScoreInputs) -> DecisionResult {
    // (0) Hot configs
    let hot_w = HOT_WEIGHTS.get_or_init(|| HotReloadWeights::new(None));
    let w = hot_w.current();
    let hot_r = HOT_RULES.get_or_init(|| HotReloadRules::new(None));
    let rules = hot_r.current();

    // (1) Initial reasons (placeholder: extend by your pipeline)
    let mut reasons: Vec<String> = Vec::new();

    // NER enrichment
    reasons = enrich_reasons(reasons, input_text);

    // (2) Base confidence via calibration weights
    let mut confidence = base_confidence(&inputs, &w);

    // Base action before rules (replace with your own signal-to-action mapping)
    let mut action = "HOLD".to_string();

    // (3) Contextual rules applied to the raw input text
    let (maybe_action, delta_conf, extra_reasons) =
        crate::analyze::rules::apply_rules_to_text(input_text, &rules);
    if let Some(a) = maybe_action {
        action = a;
    }
    confidence = (confidence + delta_conf).clamp(0.0, 1.0);
    reasons.extend(extra_reasons);

    DecisionResult {
        action,
        confidence,
        reasons,
    }
}

/// Convenience wrapper to run anti-spam filtering on a batch of (timestamp, text) items.
/// This is stateless across calls (creates a fresh AntiSpam instance) and is intended
/// for single-batch processing. For streaming, keep an `AntiSpam` instance alive and
/// call `should_block` per item.
///
/// Returns only accepted (non-blocked) items, preserving input order.
pub fn apply_antispam_batch(
    items: &[(SystemTime, String)],
    params: Option<AntiSpamParams>,
) -> Vec<(SystemTime, String)> {
    let mut anti = AntiSpam::new(params.unwrap_or_default());
    anti.filter_batch(items.iter().cloned())
}
