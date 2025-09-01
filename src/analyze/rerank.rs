// src/analyze/rerank.rs
//! Reranking: keep the last relevant statement per source and decay near-duplicates.
//!
//! - For each `source`, the **latest** (by `timestamp`) relevant statement is prioritized.
//! - Earlier statements from the same source that are *nearly identical* to the latest one
//!   get their `weight` decayed by `duplicate_decay` (default 0.7).
//!
//! Similarity: `strsim::normalized_levenshtein` (returns f64 -> cast to f32).

use std::collections::{HashMap, HashSet};
use strsim::normalized_levenshtein;

#[derive(Clone, Debug)]
pub struct Statement {
    pub source: String,
    pub timestamp: i64, // epoch-like; higher is newer
    pub text: String,
    pub weight: f32,    // mutable score to be decayed if needed
    pub relevance: f32, // 0.0..1.0 relevance score
}

impl Statement {
    pub fn is_relevant(&self, threshold: f32) -> bool {
        self.relevance >= threshold
    }
}

/// Rerank & adjust weights in-place logic, returning a **new Vec** sorted by:
/// - Latest relevant per source first,
/// - Then the remaining items (desc by timestamp).
///
/// Parameters:
/// - `relevance_threshold`
/// - `similarity_threshold`
/// - `duplicate_decay`
pub fn rerank_keep_last_and_decay_duplicates(
    mut items: Vec<Statement>,
    relevance_threshold: f32,
    similarity_threshold: f32,
    duplicate_decay: f32,
) -> Vec<Statement> {
    let mut by_source: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, it) in items.iter().enumerate() {
        by_source.entry(it.source.clone()).or_default().push(idx);
    }

    // For each source: find the latest relevant statement and decay earlier near-duplicates.
    for (_src, idxs) in by_source.iter() {
        let mut idxs_sorted = idxs.clone();
        idxs_sorted.sort_by_key(|&i| items[i].timestamp);

        let mut latest_rel_idx: Option<usize> = None;
        for &i in idxs_sorted.iter().rev() {
            if items[i].is_relevant(relevance_threshold) {
                latest_rel_idx = Some(i);
                break;
            }
        }

        if let Some(latest_idx) = latest_rel_idx {
            let latest_text = items[latest_idx].text.to_lowercase();

            for &i in idxs_sorted.iter() {
                if i == latest_idx {
                    continue;
                }
                let earlier_text = items[i].text.to_lowercase();
                let sim: f32 = normalized_levenshtein(&latest_text, &earlier_text) as f32;
                if sim >= similarity_threshold {
                    items[i].weight *= duplicate_decay;
                }
            }
        }
    }

    // Precompute the set of "TOP" items (latest relevant per source)
    let mut top_keys: HashSet<(String, i64, String)> = HashSet::new();
    for (source, idxs) in by_source.into_iter() {
        let mut latest = *idxs.iter().max_by_key(|&&i| items[i].timestamp).unwrap();
        if let Some(rel_latest) = idxs
            .iter()
            .filter(|&&i| items[i].is_relevant(relevance_threshold))
            .max_by_key(|&&i| items[i].timestamp)
            .copied()
        {
            latest = rel_latest;
        }
        let it = &items[latest];
        top_keys.insert((source, it.timestamp, it.text.clone()));
    }

    // Sort: TOP first, then the rest by timestamp desc
    items.sort_by(|a, b| {
        let a_is_top = top_keys.contains(&(a.source.clone(), a.timestamp, a.text.clone()));
        let b_is_top = top_keys.contains(&(b.source.clone(), b.timestamp, b.text.clone()));

        match (a_is_top, b_is_top) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.timestamp.cmp(&a.timestamp),
        }
    });

    items
}

/// Convenience defaults
pub const DEFAULT_RELEVANCE_THRESHOLD: f32 = 0.4;
pub const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.90;
pub const DEFAULT_DUPLICATE_DECAY: f32 = 0.7;

/// Public test wrapper so integration tests in `tests/`
/// can call a stable function without exposing internals.
#[cfg(test)]
pub mod test_api {
    use super::{
        rerank_keep_last_and_decay_duplicates, Statement, DEFAULT_DUPLICATE_DECAY,
        DEFAULT_RELEVANCE_THRESHOLD, DEFAULT_SIMILARITY_THRESHOLD,
    };

    /// Wrapper used by integration tests; forwards to the internal function.
    pub fn rerank_and_decay(
        items: Vec<Statement>,
        relevance_threshold: f32,
        similarity_threshold: f32,
        duplicate_decay: f32,
    ) -> Vec<Statement> {
        rerank_keep_last_and_decay_duplicates(
            items,
            relevance_threshold,
            similarity_threshold,
            duplicate_decay,
        )
    }

    /// Convenience overload using module defaults.
    #[allow(dead_code)]
    pub fn rerank_and_decay_with_defaults(items: Vec<Statement>) -> Vec<Statement> {
        rerank_keep_last_and_decay_duplicates(
            items,
            DEFAULT_RELEVANCE_THRESHOLD,
            DEFAULT_SIMILARITY_THRESHOLD,
            DEFAULT_DUPLICATE_DECAY,
        )
    }
}
