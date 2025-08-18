//! # Source Weights
//!
//! This module provides a configurable mapping from information sources
//! (e.g. "Trump", "Reuters", "Fed") to normalized trust/impact weights
//! in the range `[0.0, 1.0]`.
//!
//! - Loads from JSON config (weights + aliases).
//! - Case-insensitive lookup with normalization of punctuation, dashes, etc.
//! - Aliases can map alternative spellings/usernames to canonical sources.
//! - Fallback order: aliases → exact match → substring match → default.
//! - Includes a built-in `default_seed()` with common sources.
//!
//! Designed to be simple, testable, and resilient to noisy input.

use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

/// Configuration for source weights, loaded from JSON or defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceWeightsConfig {
    /// Default weight if no match is found.
    #[serde(default = "default_default_weight")]
    pub default_weight: f32,
    /// Explicit weights for canonical source names.
    #[serde(default)]
    pub weights: HashMap<String, f32>,
    /// Aliases mapping non-canonical names → canonical names.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

fn default_default_weight() -> f32 {
    0.60
}

impl SourceWeightsConfig {
    /// Load configuration from a JSON file.  
    /// Falls back to `default_seed()` on error.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| Self::default_seed()),
            Err(_) => Self::default_seed(),
        }
    }

    /// Get the weight for a given source name.
    ///
    /// Steps:
    /// 1. Alias lookup (normalized) → canonical → weight.
    /// 2. Exact weight match.
    /// 3. Substring fallback (e.g. "The Wall Street Journal" → "wall street journal").
    /// 4. Default weight.
    pub fn weight_for(&self, source: &str) -> f32 {
        let s = normalize(source);

        // 1) Alias resolution.
        if let Some(canon) = self.aliases.get(&s) {
            let c = normalize(canon);
            if let Some(&w) = self.weights.get(&c) {
                return clamp01(w);
            }
        }

        // 2) Exact weight match.
        if let Some(&w) = self.weights.get(&s) {
            return clamp01(w);
        }

        // 3) Substring fallback.
        for (k, &w) in &self.weights {
            if s.contains(k) {
                return clamp01(w);
            }
        }

        // 4) Default.
        clamp01(self.default_weight)
    }

    /// Built-in seed with common political, financial, and tech sources.
    /// Used as fallback if no config is found.
    pub(crate) fn default_seed() -> Self {
        let mut weights = HashMap::new();
        let mut aliases = HashMap::new();

        for (k, v) in [
            ("trump", 0.98),
            ("biden", 0.95),
            ("powell", 0.97),
            ("fed", 0.95),
            ("fomc", 0.95),
            ("yellen", 0.93),
            ("musk", 0.97),
            ("xi", 0.95),
            ("wsj", 0.90),
            ("wall street journal", 0.90),
            ("reuters", 0.85),
            ("bloomberg", 0.85),
            ("financial times", 0.90),
            ("new york times", 0.88),
            ("apple", 0.90),
            ("microsoft", 0.90),
            ("alphabet", 0.88),
            ("google", 0.88),
            ("meta", 0.88),
            ("facebook", 0.88),
            ("jamie dimon", 0.90),
            ("jpmorgan", 0.88),
            ("blackrock", 0.86),
            ("goldman sachs", 0.86),
            ("vanguard", 0.84),
            ("ecb", 0.86),
            ("boe", 0.84),
        ] {
            weights.insert(k.to_string(), v);
        }

        for (a, c) in [
            ("@realdonaldtrump", "trump"),
            ("president", "biden"),
            ("potus", "biden"),
            ("jerome powell", "powell"),
            ("federal reserve", "fed"),
            ("janet yellen", "yellen"),
            ("u.s. treasury", "yellen"),
            ("treasury", "yellen"),
            ("elon", "musk"),
            ("@elonmusk", "musk"),
            ("xi jinping", "xi"),
            ("xijinping", "xi"),
            ("the wall street journal", "wall street journal"),
            ("wsj.com", "wsj"),
            ("ft", "financial times"),
            ("nytimes", "new york times"),
            ("nyt", "new york times"),
            ("google inc", "google"),
            ("alphabet inc", "alphabet"),
            ("meta platforms", "meta"),
            ("facebook inc", "facebook"),
            ("jp morgan", "jpmorgan"),
            ("j.p. morgan", "jpmorgan"),
            ("gs", "goldman sachs"),
            ("european central bank", "ecb"),
            ("bank of england", "boe"),
        ] {
            aliases.insert(a.to_string(), c.to_string());
        }

        Self {
            default_weight: 0.60,
            weights,
            aliases,
        }
    }
}

/// Normalize input string: lowercase, replace punctuation/dashes with spaces,
/// collapse multiple spaces into one.
fn normalize(s: &str) -> String {
    let mut out = s.trim().to_ascii_lowercase();

    // Replace common separators with spaces.
    for ch in ['—', '–', '-', '_', '/', '\\'] {
        out = out.replace(ch, " ");
    }

    // Replace disruptive punctuation/whitespace with spaces.
    out = out.replace(['\n', '\r', '\t', '.', ',', '‚', '’', '\''], " ");

    // Collapse multiple spaces.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Clamp to [0.0, 1.0].
fn clamp01(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SourceWeightsConfig {
        SourceWeightsConfig::default_seed()
    }

    #[test]
    fn exact_match() {
        let c = cfg();
        assert!((c.weight_for("Trump") - 0.98).abs() < 1e-6);
    }

    #[test]
    fn alias_match() {
        let c = cfg();
        assert!((c.weight_for("@elonmusk") - 0.97).abs() < 1e-6);
        assert!((c.weight_for("Jerome Powell") - 0.97).abs() < 1e-6);
    }

    #[test]
    fn substring_match() {
        let c = cfg();
        assert!((c.weight_for("The Wall Street Journal") - 0.90).abs() < 1e-6);
    }

    #[test]
    fn default_weight_used() {
        let c = cfg();
        assert!((c.weight_for("TotallyUnknown") - c.default_weight).abs() < 1e-6);
    }

    #[test]
    fn case_insensitive_lookup() {
        let c = cfg();
        let a = c.weight_for("TRUMP");
        let b = c.weight_for("trump");
        let c2 = c.weight_for("Trump");
        assert!((a - b).abs() < 1e-6 && (b - c2).abs() < 1e-6);
    }

    #[test]
    fn dash_and_typography_normalization() {
        let c = cfg();
        let a = c.weight_for("Wall—Street—Journal");
        let b = c.weight_for("Wall - Street - Journal");
        let c2 = c.weight_for("The Wall Street Journal");
        assert!((a - 0.90).abs() < 1e-6);
        assert!((b - 0.90).abs() < 1e-6);
        assert!((c2 - 0.90).abs() < 1e-6);
    }

    #[test]
    fn alias_overrides_to_canonical() {
        let c = cfg();
        assert!((c.weight_for("@elonmusk") - 0.97).abs() < 1e-6);
        assert!((c.weight_for("Federal Reserve") - 0.95).abs() < 1e-6);
    }
}
