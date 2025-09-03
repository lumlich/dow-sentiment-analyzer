// src/analyze/ner.rs
//! NER extraction from JSON configs in `config/` and simple integration helpers.
//!
//! This module reads JSON files (e.g., rates.json, inflation.json, earnings.json, geopolitics.json)
//! from the `config/` directory (relative to the **current working directory**), or from a custom
//! directory set via `NER_CONFIG_DIR`. Each file contains a list of `{ regex, keyword }` patterns.
//! For any input text, matching patterns will append `"category: keyword"` entries to reasons.
//!
//! Usage:
//! - `extract_reasons_from_configs(text)` → only NER reasons.
//! - `enrich_reasons(existing, text)` → existing + NER reasons (sorted + dedup).
//!
//! Notes:
//! - Reads files on each call (fine for dev / Phase 5 Krok 1). We can add caching later.
//! - Regexes must be compatible with the `regex` crate (no lookarounds).
//! - Case-insensitive can be specified using `(?i)` in patterns.

use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Pattern {
    /// Regex string (compatible with the `regex` crate).
    pub regex: String,
    /// A short keyword to display in reasons (e.g., "CPI", "rate hike").
    pub keyword: String,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    pub patterns: Vec<Pattern>,
}

/// Resolve the directory containing NER configs:
/// - If `NER_CONFIG_DIR` is set → use it.
/// - Else use `<current_dir>/config`.
fn ner_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NER_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config")
}

/// Extracts named-entity reasons from `text` by scanning all `*.json` files in the config dir.
/// For each file, category = file stem (e.g., `inflation` for `inflation.json`).
/// Each match pushes a string `"category: keyword"`.
pub fn extract_reasons_from_configs(text: &str) -> Vec<String> {
    let mut reasons = Vec::new();

    let dir = ner_config_dir();
    let read_dir = match fs::read_dir(&dir) {
        Ok(d) => d,
        Err(_) => return reasons, // Missing dir is ok → just no reasons
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let category = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(cfg) = serde_json::from_str::<ConfigFile>(&content) else {
            continue;
        };

        for pat in cfg.patterns {
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

    // Smoke test: must preserve existing reasons and not panic without configs.
    #[test]
    fn enrich_reasons_is_stable() {
        let input = "The Fed increased interest rates to combat inflation.";
        let existing = vec!["pipeline: base reason".to_string()];
        let out = enrich_reasons(existing, input);

        assert!(out.iter().any(|s| s == "pipeline: base reason"));
        // No strict assertion on NER presence (depends on local config files).
    }

    // Optional tiny check that empty / missing config dir yields empty NER reasons.
    #[test]
    fn extract_empty_when_no_config_dir() {
        // Point to a definitely-nonexistent dir (random suffix)
        std::env::set_var("NER_CONFIG_DIR", "__ner_config_dir_should_not_exist__");
        let out = extract_reasons_from_configs("anything");
        assert!(out.is_empty());
        // cleanup
        std::env::remove_var("NER_CONFIG_DIR");
    }
}
