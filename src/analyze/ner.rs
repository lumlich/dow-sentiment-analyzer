// src/analyze/ner.rs
//! NER extraction from JSON configs and simple integration helpers.
//!
//! - Looks for *.json in a config dir (NER_CONFIG_DIR or ./config).
//! - Each JSON: {"patterns":[{"regex":"...","keyword":"..."}]}
//! - For each match, adds "category: keyword" where category = file stem (e.g. rates.json -> "rates").
//!
//! Notes:
//! - Reads files on every call (fine for dev/CI). Caching lze přidat později.
//! - Regex kompatibilní s crate `regex` (bez lookaround), case-insensitive přes `(?i)`.

use regex::Regex;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct Pattern {
    /// Regex string (compatible with the `regex` crate).
    pub regex: String,
    /// Short keyword to display in reasons (e.g., "CPI", "rate hike").
    pub keyword: String,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    pub patterns: Vec<Pattern>,
}

fn candidate_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(s) = std::env::var("NER_CONFIG_DIR") {
        if !s.trim().is_empty() {
            dirs.push(PathBuf::from(s));
        }
    }
    // Fallback na relativní ./config vůči aktuálnímu CWD
    dirs.push(PathBuf::from("config"));

    dirs
}

/// Extracts named-entity reasons from `text` using JSON configs.
/// Each match pushes a string in the form `"category: keyword"`.
pub fn extract_reasons_from_configs(text: &str) -> Vec<String> {
    let mut reasons = Vec::new();

    // Projdi kandidátní adresáře v pořadí (ENV -> ./config).
    // Jakmile v jednom něco najdeme, další už není třeba číst (zamezí duplicitám).
    for dir in candidate_config_dirs() {
        if !dir.exists() {
            continue;
        }

        let read_dir = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        let mut local_found = false;

        for entry in read_dir.flatten() {
            let path = entry.path();

            // Jen *.json soubory
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if !is_json {
                continue;
            }

            // Kategorie = stem souboru (rates.json -> "rates")
            let Some(category) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let cfg: ConfigFile = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue, // tichý skip rozbitých configů
            };

            for pat in cfg.patterns {
                if let Ok(re) = Regex::new(&pat.regex) {
                    if re.is_match(text) {
                        reasons.push(format!("{category}: {}", pat.keyword));
                        local_found = true;
                    }
                }
            }
        }

        if local_found {
            break; // našli jsme něco v tomto diru; další diry už nečteme
        }
    }

    reasons
}

/// Enrich an existing reasons vector with NER reasons extracted from `text`.
/// The result is sorted and deduplicated.
pub fn enrich_reasons(mut existing_reasons: Vec<String>, text: &str) -> Vec<String> {
    let mut ner = extract_reasons_from_configs(text);
    existing_reasons.append(&mut ner);

    existing_reasons.sort();
    existing_reasons.dedup();

    existing_reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_empty_when_no_config_dir() {
        // Bez ENV a bez ./config by mělo vrátit prázdno
        std::env::remove_var("NER_CONFIG_DIR");
        assert!(extract_reasons_from_configs("anything").is_empty());
    }

    #[test]
    fn enrich_reasons_is_stable() {
        let input = "The Fed increased interest rates to combat inflation.";
        let existing = vec!["pipeline: base reason".to_string()];
        let out = enrich_reasons(existing, input);

        // jen sanity — zachováme existující důvody
        assert!(out.iter().any(|s| s == "pipeline: base reason"));
    }
}
