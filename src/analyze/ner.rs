// src/analyze/ner.rs
//! NER extraction from JSON configs in `config/` and simple integration helpers.
//!
//! This module reads JSON files (rates.json, inflation.json, earnings.json, geopolitics.json)
//! from the `config/` directory. Each file contains a list of { regex, keyword } patterns.
//! For any input text, matching patterns will append `"category: keyword"` entries to reasons.
//!
//! Usage patterns:
//! - Call `extract_reasons_from_configs(text)` to get only NER reasons.
//! - Or call `enrich_reasons(existing_reasons, text)` to merge NER reasons into an existing list
//!   (sorted + deduplicated).
//!
//! Notes:
//! - This implementation reads files on each call (good for dev / Krok 1).
//!   Caching/hot-reload can be added later (Krok 4).
//! - Regexes must be compatible with the `regex` crate (no lookarounds).
//! - Case-insensitive can be specified using `(?i)` in patterns.

use regex::Regex;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
struct Pattern {
    /// Regex string (compatible with the `regex` crate).
    pub regex: String,
    /// A short keyword to display in reasons (e.g., "CPI", "rate hike").
    pub keyword: String,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    pub patterns: Vec<Pattern>,
}

/// (category, file_path)
const CATEGORIES: &[(&str, &str)] = &[
    ("rates", "config/rates.json"),
    ("inflation", "config/inflation.json"),
    ("earnings", "config/earnings.json"),
    ("geopolitics", "config/geopolitics.json"),
];

/// Extracts named-entity reasons from `text` using JSON configs in `/config`.
/// Each match pushes a string in the form `"category: keyword"`.
pub fn extract_reasons_from_configs(text: &str) -> Vec<String> {
    let mut reasons = Vec::new();

    for (category, path) in CATEGORIES {
        if !Path::new(path).exists() {
            // Gracefully skip missing config files to keep dev flow smooth.
            continue;
        }

        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(cfg) = serde_json::from_str::<ConfigFile>(&content) else {
            continue;
        };

        for pat in cfg.patterns {
            // Compile regex; skip invalid patterns to avoid crashing prod/dev flow.
            if let Ok(re) = Regex::new(&pat.regex) {
                if re.is_match(text) {
                    reasons.push(format!("{category}: {}", pat.keyword));
                }
            }
        }
    }

    reasons
}

/// Enrich an existing reasons vector with NER reasons extracted from `text`.
/// The result is sorted and deduplicated.
pub fn enrich_reasons(mut existing_reasons: Vec<String>, text: &str) -> Vec<String> {
    let mut ner = extract_reasons_from_configs(text);
    existing_reasons.append(&mut ner);

    // Light dedup to avoid exact duplicates at this stage.
    existing_reasons.sort();
    existing_reasons.dedup();

    existing_reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are smoke-style tests; they only check function shape/end-to-end flow with minimal assumptions.
    // They won't run unless you enable tests. Keep them here for later larger test batches if desired.

    #[test]
    fn enrich_reasons_is_stable() {
        let input = "The Fed increased interest rates to combat inflation.";
        let existing = vec!["pipeline: base reason".to_string()];
        let out = enrich_reasons(existing, input);

        // We don't assert exact contents (depends on local JSON files),
        // only that we don't crash and we keep at least the existing reason.
        assert!(out.iter().any(|s| s == "pipeline: base reason"));
    }
}
