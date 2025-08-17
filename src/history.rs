//! history.rs — jednoduché in-memory logování rozhodnutí pro budoucí antiflutter/alerts.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::decision::{Decision, Verdict};

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub ts_unix: u64,
    pub verdict: Verdict,
    pub confidence: f32,
    // stručné „otisky“ explainability pro rychlou diagnostiku:
    pub top_sources: Vec<String>, // např. ["Trump", "Fed"]
    pub top_scores: Vec<i32>,     // např. [2, -3]
}

#[derive(Debug)]
pub struct History {
    inner: Mutex<Vec<HistoryEntry>>,
    cap: usize,
}

impl History {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::with_capacity(cap.min(10_000))),
            cap: cap.min(10_000),
        }
    }

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

    pub fn snapshot_last_n(&self, n: usize) -> Vec<HistoryEntry> {
        let v = self.inner.lock().expect("history mutex poisoned");
        let len = v.len();
        let start = len.saturating_sub(n);
        v[start..].to_vec()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}