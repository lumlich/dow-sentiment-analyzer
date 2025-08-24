//! # Decision Model
//! Structures for verdicts, explainability, and top contributors.
//!
//! ## Purpose
//! Provide a standardized output shape for `BUY` / `HOLD` / `SELL` with a
//! normalized `confidence` and human-readable `reasons`, so we can plug in
//! rolling context, disruption detection, and confidence calibration without
//! changing the public API.
//!
//! Notes: The app prioritizes *disruptive* statements (shocks). Rolling metrics
//! are informative; alerts are ultimately triggered by disruption logic.

use serde::{Deserialize, Serialize};

// ----- Relevance gate hook (light coupling) -----
use crate::relevance::RelevanceHandle;
use sha2::{Digest, Sha256};

/// Final verdict of the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Buy,
    Hold,
    Sell,
}

/// A concise reason shown to users (explainability).
/// Keep it readable; we can refine categories as the system evolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    /// Human-readable explanation (e.g., "Trump said economy is strong (+2)").
    pub message: String,
    /// Optional weight in `<0.0, 1.0>` when meaningful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    /// Optional category to keep UI/tests consistent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ReasonKind>,
}

/// Coarse-grained reason kinds (for UI/test cohesion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    SourceStrength,
    Recency,
    Consensus,
    Volume,
    RollingTrend,
    Threshold,
    Other,
}

/// Top contributors to the current verdict.
/// Lets us show "evidence": who said what, with what score, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contributor {
    /// E.g., "Trump", "Fed", "Yellen", "Analyst", ...
    pub source: String,
    /// Original short text of the statement/news item.
    pub text: String,
    /// Final lexicon score (aggregate integer).
    pub score: i32,
    /// Statement timestamp — ISO 8601 preferred (e.g., "2025-08-16T10:00:00Z").
    /// Kept as `String` to avoid adding chrono; filled during processing.
    #[serde(rename = "ts")]
    pub ts_iso: String,

    /// Optional partial weights used by disruption detection.
    /// Added for future explainability; may be absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_source: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_recency: Option<f32>,
}

/// Complete decision including explainability.
/// This is the shape returned by the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub decision: Verdict,
    /// Confidence in `<0.0, 1.0>`.
    pub confidence: f32,
    /// Short but useful list of reasons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<Reason>,
    /// Top N contributors (typically 1–3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_contributors: Vec<Contributor>,
}

#[allow(dead_code)]
impl Decision {
    /// Create a skeletal decision with the given verdict and confidence.
    pub fn new(verdict: Verdict, confidence: f32) -> Self {
        Self {
            decision: verdict,
            confidence: clamp01(confidence),
            reasons: Vec::new(),
            top_contributors: Vec::new(),
        }
    }

    /// Convenience constructors.
    pub fn buy(confidence: f32) -> Self {
        Self::new(Verdict::Buy, confidence)
    }
    pub fn hold(confidence: f32) -> Self {
        Self::new(Verdict::Hold, confidence)
    }
    pub fn sell(confidence: f32) -> Self {
        Self::new(Verdict::Sell, confidence)
    }

    /// Add a single reason (builder style).
    pub fn with_reason(mut self, message: impl Into<String>) -> Self {
        self.reasons.push(Reason {
            message: message.into(),
            weight: None,
            kind: None,
        });
        self
    }

    /// Add a contributor (builder style).
    pub fn with_contributor(mut self, c: Contributor) -> Self {
        self.top_contributors.push(c);
        self
    }

    /// Apply the relevance gate to this decision.
    ///
    /// Contract:
    /// - If the relevance score is neutralized (<= 0.0), set confidence to 0.0 and
    ///   append a threshold-kind reason. Keep the original verdict for transparency.
    /// - If the relevance score is positive, append a passing reason with the score.
    ///
    /// Logging:
    /// - Dev-only tracing: anonymized text hash, short matched list, and first reason.
    pub fn apply_relevance_gate(
        &mut self,
        input_text: &str,
        handle: &RelevanceHandle,
    ) {
        let rel = handle.score(input_text);
        let passed = rel.score > 0.0;

        // Human-facing reason
        if passed {
            self.reasons.push(
                Reason::new(format!("relevance gate passed (rel {:.2})", rel.score))
                    .kind(ReasonKind::Threshold),
            );
        } else {
            self.confidence = 0.0; // neutralize confidence
            self.reasons.push(
                Reason::new("neutralized by relevance gate (rel <= 0.00)")
                    .kind(ReasonKind::Threshold),
            );
        }

        // Dev-only anonymized logs (activated via main.rs init)
        let first_reason = rel.reasons.get(0).cloned().unwrap_or_default();
        let matched_short = truncate_vec(&rel.matched, 8);
        let hash = anon_hash_short(input_text);

        if passed {
            tracing::debug!(
                target: "relevance",
                evt = "passed",
                rel_score = %format!("{:.2}", rel.score),
                matched = ?matched_short,
                reason0 = %first_reason,
                hash = %hash,
                "relevance gate evaluation"
            );
        } else {
            tracing::info!(
                target: "relevance",
                evt = "neutralized",
                rel_score = %format!("{:.2}", rel.score),
                matched = ?matched_short,
                reason0 = %first_reason,
                hash = %hash,
                "relevance gate evaluation"
            );
        }

        // Optionally surface raw reasons from relevance (useful for introspection)
        // Comment this out if you do not want to leak internal rationale to clients.
        for r in rel.reasons {
            self.reasons.push(Reason::new(format!("rel: {}", r)));
        }
    }
}

impl Reason {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            weight: None,
            kind: None,
        }
    }

    pub fn weighted(mut self, w: f32) -> Self {
        self.weight = Some(clamp01(w));
        self
    }

    pub fn kind(mut self, kind: ReasonKind) -> Self {
        self.kind = Some(kind);
        self
    }
}

impl Contributor {
    pub fn new(
        source: impl Into<String>,
        text: impl Into<String>,
        score: i32,
        ts_iso: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            text: text.into(),
            score,
            ts_iso: ts_iso.into(),
            w_source: None,
            w_strength: None,
            w_recency: None,
        }
    }

    pub fn weights(mut self, w_source: f32, w_strength: f32, w_recency: f32) -> Self {
        self.w_source = Some(clamp01(w_source));
        self.w_strength = Some(clamp01(w_strength));
        self.w_recency = Some(clamp01(w_recency));
        self
    }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Produce a short anonymized hash for dev logs (first 12 hex chars).
fn anon_hash_short(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(12);
    for b in digest.iter().take(6) {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

/// Truncate any list-like view for compact logging.
fn truncate_vec<T: ToString>(v: &[T], max: usize) -> Vec<String> {
    v.iter().take(max).map(|x| x.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_decision_shape_matches_contract() {
        let d = Decision::buy(0.78)
            .with_reason("Trump said economy is strong (+2)")
            .with_reason("Fed cautious (-1)")
            .with_contributor(
                Contributor::new("Trump", "The economy is strong.", 2, "2025-08-16T10:00:00Z")
                    .weights(0.95, 0.92, 1.0),
            );

        let v: serde_json::Value = serde_json::to_value(&d).unwrap();

        // Key fields per contract
        assert_eq!(v["decision"], serde_json::json!("BUY"));

        // Compare floats with tolerance
        let conf = v["confidence"].as_f64().unwrap();
        assert!(
            (conf - 0.78).abs() < 1e-6,
            "confidence ~= 0.78, got {}",
            conf
        );

        assert!(v["reasons"].is_array());
        assert!(v["top_contributors"].is_array());

        // At least one contributor with expected fields
        let c = &v["top_contributors"][0];
        assert_eq!(c["source"], serde_json::json!("Trump"));
        assert_eq!(c["text"], serde_json::json!("The economy is strong."));
        assert_eq!(c["score"], serde_json::json!(2));
        assert_eq!(c["ts"], serde_json::json!("2025-08-16T10:00:00Z"));
    }
}
