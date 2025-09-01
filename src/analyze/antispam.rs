//! Anti-spam sliding-window filter.
//!
//! Goal: suppress near-duplicate texts that arrive within a short time window.
//!
//! Simple API, no external crates:
//! - Configure with `AntiSpamParams { window_size, similarity_threshold, time_window_secs }`
//! - Call `should_block(ts, text)` for stream processing: returns `true` if the item
//!   should be filtered out (spam/near-duplicate), otherwise `false` (and the item is remembered)
//! - Optionally call `filter_batch(items)` to keep only non-blocked items in one pass.
//!
//! Similarity metric: normalized Levenshtein similarity in [0.0, 1.0].
//! An item is considered spam if there exists any recent (within the time window) remembered text
//! whose similarity >= `similarity_threshold`.
//!
//! NOTE: This module is intentionally self-contained and zero-deps.

use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

/// Configuration for the anti-spam filter.
#[derive(Clone, Debug)]
pub struct AntiSpamParams {
    /// Max number of remembered items (capacity of the sliding window).
    pub window_size: usize,
    /// Similarity in [0.0, 1.0]. Items >= this threshold are considered "near-duplicates".
    pub similarity_threshold: f32,
    /// Time window in seconds; only items newer than (ts - time_window_secs) are considered.
    pub time_window_secs: u64,
}

impl Default for AntiSpamParams {
    fn default() -> Self {
        Self {
            window_size: 128,
            similarity_threshold: 0.90,
            time_window_secs: 10 * 60, // 10 minutes
        }
    }
}

#[derive(Clone, Debug)]
struct SeenItem {
    ts: SystemTime,
    text: String,
}

/// In-memory sliding-window anti-spam filter.
#[derive(Debug)]
pub struct AntiSpam {
    params: AntiSpamParams,
    window: VecDeque<SeenItem>,
}

impl AntiSpam {
    /// Create a new filter with given params.
    pub fn new(mut params: AntiSpamParams) -> Self {
        // Basic parameter hygiene
        if params.window_size == 0 {
            params.window_size = 1;
        }
        params.similarity_threshold = params.similarity_threshold.clamp(0.0, 1.0);
        if params.time_window_secs == 0 {
            params.time_window_secs = 1;
        }

        // Save capacity before moving params
        let ws = params.window_size;

        Self {
            params,
            window: VecDeque::with_capacity(ws),
        }
    }

    /// Get immutable reference to params.
    pub fn params(&self) -> &AntiSpamParams {
        &self.params
    }

    /// Update parameters at runtime (keeps current memory/window).
    pub fn set_params(&mut self, params: AntiSpamParams) {
        let mut p = params;
        if p.window_size == 0 {
            p.window_size = 1;
        }
        p.similarity_threshold = p.similarity_threshold.clamp(0.0, 1.0);
        if p.time_window_secs == 0 {
            p.time_window_secs = 1;
        }
        self.params = p;
        // Shrink if needed
        while self.window.len() > self.params.window_size {
            self.window.pop_front();
        }
    }

    /// Clears the remembered sliding window.
    pub fn clear(&mut self) {
        self.window.clear();
    }

    /// Decide whether to block the given text observed at `ts`.
    pub fn should_block(&mut self, ts: SystemTime, text: &str) -> bool {
        let norm_text = normalize(text);
        self.evict_old(ts);

        // Check against recent memory
        for item in self.window.iter().rev() {
            let sim = normalized_levenshtein(&norm_text, &item.text);
            if sim >= self.params.similarity_threshold {
                return true;
            }
        }

        // Otherwise accept and remember the item
        self.remember(ts, norm_text);
        false
    }

    /// Batch helper: keeps only non-blocked items, in order.
    pub fn filter_batch<I, S>(&mut self, items: I) -> Vec<(SystemTime, S)>
    where
        I: IntoIterator<Item = (SystemTime, S)>,
        S: AsRef<str> + Clone,
    {
        let mut out = Vec::new();
        for (ts, s) in items {
            if !self.should_block(ts, s.as_ref()) {
                out.push((ts, s));
            }
        }
        out
    }

    // -- internals --

    fn remember(&mut self, ts: SystemTime, norm_text: String) {
        if self.window.len() == self.params.window_size {
            self.window.pop_front();
        }
        self.window.push_back(SeenItem {
            ts,
            text: norm_text,
        });
    }

    fn evict_old(&mut self, now: SystemTime) {
        let horizon = Duration::from_secs(self.params.time_window_secs);
        while let Some(front) = self.window.front() {
            if now
                .duration_since(front.ts)
                .unwrap_or_else(|_| Duration::from_secs(0))
                > horizon
            {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Normalize text before similarity
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for ch in s.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(lc);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

/// Compute normalized Levenshtein similarity
fn normalized_levenshtein(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein(a, b) as f32;
    1.0 - (dist / max_len as f32)
}

/// Standard Levenshtein distance
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];

    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[m]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[allow(dead_code)]
    fn ts(sec: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000 + sec)
    }

    // … testy beze změn …
}
