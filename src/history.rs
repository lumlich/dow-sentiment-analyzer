//! # History (In-Memory Decision Log)
//! Simple in-memory logging of recent `Decision`s for diagnostics and
//! potential future anti-flutter/alert logic.
//!
//! - Capacity-limited circular buffer (max 10,000).
//! - Stores verdict, confidence, and lightweight contributor fingerprints
//!   (sources and scores).
//! - Intended for quick debugging, transparency, and context checks.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::decision::{Decision, Verdict};

/// Compact record of a past decision.
/// Used for quick lookback (no full explainability retained).
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub ts_unix: u64,
    pub verdict: Verdict,
    pub confidence: f32,
    /// Top contributor sources (e.g., `["Trump", "Fed"]`).
    pub top_sources: Vec<String>,
    /// Their corresponding scores (e.g., `[2, -3]`).
    pub top_scores: Vec<i32>,
}

/// Fixed-capacity in-memory buffer of past decisions.
/// Thread-safe with a simple `Mutex`.
#[derive(Debug)]
pub struct History {
    inner: Mutex<Vec<HistoryEntry>>,
    cap: usize,
}

impl History {
    /// Create a new `History` with the given maximum capacity (capped at 10k).
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::with_capacity(cap.min(10_000))),
            cap: cap.min(10_000),
        }
    }

    /// Append a decision snapshot to history.
    ///
    /// Keeps only the last `cap` entries; drains older items if needed.
    pub fn push(&self, d: &Decision) {
        let ts = now_unix();
        let (sources, scores) = {
            let mut s = Vec::new();
            let mut sc = Vec::new();
            for c in d.top_contributors.iter().take(3) {
                s.push(c.source.clone());
                sc.push(c.score);
            }
            (s, sc)
        };

        let entry = HistoryEntry {
            ts_unix: ts,
            verdict: d.decision,
            confidence: d.confidence,
            top_sources: sources,
            top_scores: scores,
        };

        let mut v = self.inner.lock().expect("history mutex poisoned");
        v.push(entry);
        if v.len() > self.cap {
            let excess = v.len() - self.cap;
            v.drain(0..excess);
        }
    }

    /// Return a snapshot of the last `n` entries (cheap clone of stored slice).
    pub fn snapshot_last_n(&self, n: usize) -> Vec<HistoryEntry> {
        let v = self.inner.lock().expect("history mutex poisoned");
        let len = v.len();
        let start = len.saturating_sub(n);
        v[start..].to_vec()
    }
}

/// Current UNIX timestamp in seconds.
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
