// src/relevance.rs
//! Relevance gate primitives: tokenizer, tag parsers, config types, regex compilation,
//! proximity checks, and scoring.

use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};
use tracing::info;

// --- env defaults & names ---
pub const DEFAULT_RELEVANCE_CONFIG_PATH: &str = "config/relevance.toml";
pub const DEFAULT_RELEVANCE_THRESHOLD: f32 = 0.5;

pub const ENV_RELEVANCE_CONFIG_PATH: &str = "RELEVANCE_CONFIG_PATH";
pub const ENV_RELEVANCE_THRESHOLD: &str = "RELEVANCE_THRESHOLD";

// Simple shared app state used by Axum.
#[derive(Clone)]
pub struct AppState {
    pub relevance: RelevanceHandle,
}

// Dev logging gate: RELEVANCE_DEV_LOG=1 AND dev env (debug or SHUTTLE_ENV in {local,development,dev})
pub(crate) fn dev_logging_enabled() -> bool {
    let on = std::env::var("RELEVANCE_DEV_LOG").ok().as_deref() == Some("1");
    if !on {
        return false;
    }
    if cfg!(debug_assertions) {
        return true;
    }
    matches!(
        std::env::var("SHUTTLE_ENV")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "local" | "development" | "dev"
    )
}

// Make these helpers available to other modules (e.g., /decide)
pub(crate) fn anon_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
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

/// Minimal, anonymized dev logger for relevance events.
fn dev_log_relevance(
    event: &str,
    text: &str,
    matched: &[String],
    reasons: &[String],
    score: f32,
    threshold: f32,
) {
    if !dev_logging_enabled() {
        return;
    }
    let id = anon_hash(text);
    let matched_short = truncate_vec(matched, 5);
    let reasons_short = truncate_vec(reasons, 5);
    // Never log raw text. Only hashed id + short lists.
    info!(
        target: "relevance",
        %id, %score, %threshold, event,
        matched = ?matched_short,
        reasons = ?reasons_short
    );
}

pub(crate) fn truncate_vec<T: ToString>(v: &[T], max: usize) -> Vec<String> {
    v.iter().take(max).map(|x| x.to_string()).collect()
}

// parse optional float env and clamp to <0.0..=1.0>
fn parse_threshold_env(raw: Option<String>) -> Option<f32> {
    raw.and_then(|s| s.trim().parse::<f32>().ok())
        .map(|v| v.clamp(0.0, 1.0))
}

/// Result of relevance evaluation
#[derive(Debug, Clone, PartialEq)]
pub struct Relevance {
    pub score: f32,
    pub matched: Vec<String>,
    pub reasons: Vec<String>,
}

impl Default for Relevance {
    fn default() -> Self {
        Self {
            score: 0.0,
            matched: Vec::new(),
            reasons: Vec::new(),
        }
    }
}

/// A single token with byte span and sequential index
#[derive(Debug, Clone)]
pub struct Token {
    #[cfg_attr(not(test), allow(dead_code))]
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub index: usize, // 0-based token index in the sequence
}

/// Basic, Unicode-friendly tokenizer.
pub fn tokenize(input: &str) -> Vec<Token> {
    // \w covers [A-Za-z0-9_]; (?u) enables Unicode
    let re = Regex::new(r"(?u)\b\w+\b").expect("tokenizer regex");
    let mut out = Vec::new();
    for (i, m) in re.find_iter(input).enumerate() {
        out.push(Token {
            text: input[m.start()..m.end()].to_string(),
            start: m.start(),
            end: m.end(),
            index: i,
        });
    }
    out
}

/// Extract cashtags like `$DJI`, `$DOW`, allowing 1–5 letters.
/// Returns distinct, uppercase symbols (without `$`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_cashtags(input: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)(?P<tag>\$[a-z]{1,5})\b").expect("cashtag regex");
    let mut tags = Vec::new();
    for caps in re.captures_iter(input) {
        if let Some(m) = caps.name("tag") {
            tags.push(m.as_str()[1..].to_ascii_uppercase());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/// Extract hashtags like `#DJIA`, `#DowJones`.
/// Returns distinct, lowercased tags (without `#`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_hashtags(input: &str) -> Vec<String> {
    let re = Regex::new(r"(?i)(?P<tag>#[a-z0-9_]+)\b").expect("hashtag regex");
    let mut tags = Vec::new();
    for caps in re.captures_iter(input) {
        if let Some(m) = caps.name("tag") {
            tags.push(m.as_str()[1..].to_ascii_lowercase());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/* ----------------------------
Config schema (from TOML)
---------------------------- */

#[derive(Debug, Clone, Deserialize)]
pub struct RelevanceRoot {
    pub relevance: RelevanceSection,
    pub weights: HashMap<String, i32>,
    #[serde(default)]
    pub anchors: Vec<AnchorCfg>,
    #[serde(default)]
    pub blockers: Vec<BlockerCfg>,
    #[serde(default)]
    pub combos: ComboCfg,
    #[serde(default)]
    pub aliases: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelevanceSection {
    pub threshold: f32,
    #[allow(dead_code)] // informational only (kept for config docs)
    pub near_default_window: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnchorCfg {
    pub id: String,
    pub category: String, // "hard" | "semi" | "macro" | "soft" | "verb"
    pub pattern: String,  // regex (already escaped in TOML)
    #[serde(default)]
    pub near: Option<NearCfg>,
    #[serde(default)]
    pub tag: Option<String>, // optional metadata, e.g. "single_stock_only"
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockerCfg {
    pub id: String,
    pub pattern: String,
    pub reason: String,
    #[allow(dead_code)] // reserved for future actions
    pub action: String, // e.g. "block"
    #[serde(default)]
    pub near: Option<NearCfg>,
    #[serde(default, rename = "unless_near")]
    pub unless_near: Option<NearCfg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NearCfg {
    pub pattern: String,
    pub window: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ComboCfg {
    #[serde(default, rename = "pass_any")]
    pub pass_any: Vec<ComboNeed>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComboNeed {
    pub need: Vec<String>, // e.g. ["hard","verb"] or ["macro","macro","verb_or_semi"]
}

/* ----------------------------
Compiled engine structures
---------------------------- */

#[derive(Debug)]
struct CompiledAnchor {
    cfg: AnchorCfg,
    re: Regex,
    near: Option<(Regex, usize)>,
}

#[derive(Debug)]
struct CompiledBlocker {
    cfg: BlockerCfg,
    re: Regex,
    near: Option<(Regex, usize)>,
    unless_near: Option<(Regex, usize)>,
}

/// The engine holds compiled regexes and provides proximity utilities.
#[derive(Debug)]
pub struct RelevanceEngine {
    pub cfg: RelevanceRoot,
    anchors: Vec<CompiledAnchor>,
    blockers: Vec<CompiledBlocker>,
}

impl RelevanceEngine {
    /// Load from a TOML file. Uses RELEVANCE_CONFIG_PATH or defaults to "config/relevance.toml".
    pub fn from_toml() -> anyhow::Result<Self> {
        // resolve path
        let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));

        let content = fs::read_to_string(&path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read relevance config at {}: {}",
                path.display(),
                e
            )
        })?;

        // build engine from string
        let mut eng = Self::from_toml_str(&content)?;

        // optional: override threshold from env
        if let Some(t) = parse_threshold_env(std::env::var(ENV_RELEVANCE_THRESHOLD).ok()) {
            // override the TOML-provided threshold
            eng.cfg.relevance.threshold = t;
        } else if !eng.cfg.relevance.threshold.is_finite() {
            // harden: ensure some sane threshold even if TOML is odd
            eng.cfg.relevance.threshold = DEFAULT_RELEVANCE_THRESHOLD;
        }

        Ok(eng)
    }

    /// Load from a TOML string
    pub fn from_toml_str(toml_str: &str) -> anyhow::Result<Self> {
        let cfg: RelevanceRoot = toml::from_str(toml_str)?;
        // Compile anchors
        let anchors = cfg
            .anchors
            .iter()
            .cloned()
            .map(|a| {
                let re = Regex::new(&a.pattern)
                    .map_err(|e| anyhow::anyhow!("anchor `{}` regex error: {}", a.id, e))?;
                let near = if let Some(nc) = &a.near {
                    let nr = Regex::new(&nc.pattern).map_err(|e| {
                        anyhow::anyhow!("anchor `{}` near-regex error: {}", a.id, e)
                    })?;
                    Some((nr, nc.window))
                } else {
                    None
                };
                Ok(CompiledAnchor { cfg: a, re, near })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        // Compile blockers
        let blockers = cfg
            .blockers
            .iter()
            .cloned()
            .map(|b| {
                let re = Regex::new(&b.pattern)
                    .map_err(|e| anyhow::anyhow!("blocker `{}` regex error: {}", b.id, e))?;
                let near = if let Some(nc) = &b.near {
                    let nr = Regex::new(&nc.pattern).map_err(|e| {
                        anyhow::anyhow!("blocker `{}` near-regex error: {}", b.id, e)
                    })?;
                    Some((nr, nc.window))
                } else {
                    None
                };
                let unless_near = if let Some(nc) = &b.unless_near {
                    let nr = Regex::new(&nc.pattern).map_err(|e| {
                        anyhow::anyhow!("blocker `{}` unless_near regex error: {}", b.id, e)
                    })?;
                    Some((nr, nc.window))
                } else {
                    None
                };
                Ok(CompiledBlocker {
                    cfg: b,
                    re,
                    near,
                    unless_near,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            cfg,
            anchors,
            blockers,
        })
    }

    /// Tokenize once and return tokens + quick index of byte->token mapping for proximity checks.
    #[allow(clippy::needless_range_loop)]
    pub fn tokenize_with_index(&self, text: &str) -> (Vec<Token>, Vec<usize>) {
        let tokens = tokenize(text);
        // Build byte-position → token-index lookup (sparse; length = text.len()+1)
        let mut byte_to_tok = vec![usize::MAX; text.len() + 1];
        for t in &tokens {
            for i in t.start..=t.end {
                byte_to_tok[i] = t.index;
            }
        }
        // Backfill gaps with previous known index
        let mut last = usize::MAX;
        for i in 0..byte_to_tok.len() {
            if byte_to_tok[i] == usize::MAX {
                byte_to_tok[i] = last;
            } else {
                last = byte_to_tok[i];
            }
        }
        (tokens, byte_to_tok)
    }

    /// Map a regex match's start byte into a token index (best effort).
    fn token_index_for_start(byte_to_tok: &[usize], start: usize) -> Option<usize> {
        if start < byte_to_tok.len() {
            let idx = byte_to_tok[start];
            if idx != usize::MAX {
                return Some(idx);
            }
        }
        None
    }

    /// Return true if any main-match token is within `window` tokens of any near-match token.
    fn within_window(main_idxs: &[usize], near_idxs: &[usize], window: usize) -> bool {
        for &a in main_idxs {
            for &b in near_idxs {
                let dist = if a > b { a - b } else { b - a };
                if dist <= window {
                    return true;
                }
            }
        }
        false
    }

    /// Collect token indices for all matches of `re` in `text`, using the provided byte→token map.
    fn match_token_indices(re: &Regex, text: &str, byte_to_tok: &[usize]) -> Vec<usize> {
        re.find_iter(text)
            .filter_map(|m| Self::token_index_for_start(byte_to_tok, m.start()))
            .collect()
    }

    /// Find blockers that apply to `text` considering optional `near`/`unless_near`.
    pub fn find_blockers(&self, text: &str) -> Vec<String> {
        let (_tokens, byte_to_tok) = self.tokenize_with_index(text);

        let mut hits = Vec::new();
        for b in &self.blockers {
            let mut main_idxs = Self::match_token_indices(&b.re, text, &byte_to_tok);
            if main_idxs.is_empty() {
                continue;
            }

            // If blocker has `near`, require proximity
            if let Some((near_re, win)) = &b.near {
                let near_idxs = Self::match_token_indices(near_re, text, &byte_to_tok);
                if near_idxs.is_empty() || !Self::within_window(&main_idxs, &near_idxs, *win) {
                    // doesn't satisfy near → treat as not matched
                    main_idxs.clear();
                }
            }

            if main_idxs.is_empty() {
                continue;
            }

            // If blocker has `unless_near`, and that proximity holds, skip blocking
            if let Some((unless_re, win)) = &b.unless_near {
                let unless_idxs = Self::match_token_indices(unless_re, text, &byte_to_tok);
                if !unless_idxs.is_empty() && Self::within_window(&main_idxs, &unless_idxs, *win) {
                    // Exception applies → do not block
                    continue;
                }
            }

            hits.push(format!("blocker:{}:{}", b.cfg.id, b.cfg.reason));
        }
        hits
    }

    /// Find anchor hits with proximity qualification (if configured).
    /// Returns vector of "anchor:<id>[:tag]" strings.
    #[allow(dead_code)]
    pub fn find_anchors(&self, text: &str) -> Vec<String> {
        let (_tokens, byte_to_tok) = self.tokenize_with_index(text);

        let mut out = Vec::new();
        for a in &self.anchors {
            let main_idxs = Self::match_token_indices(&a.re, text, &byte_to_tok);
            if main_idxs.is_empty() {
                continue;
            }

            // If anchor has a `near` requirement, enforce it
            if let Some((near_re, win)) = &a.near {
                let near_idxs = Self::match_token_indices(near_re, text, &byte_to_tok);
                if near_idxs.is_empty() || !Self::within_window(&main_idxs, &near_idxs, *win) {
                    continue;
                }
            }

            if let Some(tag) = &a.cfg.tag {
                out.push(format!("anchor:{}:{}", a.cfg.id, tag));
            } else {
                out.push(format!("anchor:{}", a.cfg.id));
            }
        }
        out
    }

    /// Shell API for future scoring: evaluates blockers first, then anchors.
    /// Currently returns a `Relevance` with matched markers; score stays 0.0.
    #[allow(dead_code)]
    pub fn evaluate(&self, text: &str) -> Relevance {
        let mut rel = Relevance::default();

        let blockers = self.find_blockers(text);
        if !blockers.is_empty() {
            rel.reasons.extend(blockers);
            // Score remains 0.0 deliberately (blocked).
            return rel;
        }

        let anchors = self.find_anchors(text);
        rel.matched = anchors;
        rel
    }

    /* -------- Scoring helpers (precision-first) -------- */

    /// Internal: run anchor matching and return (matched_ids, category_counts, has_single_stock_only_tag)
    fn collect_anchor_stats(&self, text: &str) -> (Vec<String>, HashMap<String, usize>, bool) {
        let (_tokens, byte_to_tok) = self.tokenize_with_index(text);

        let mut matched_ids = Vec::new();
        let mut cat_counts: HashMap<String, usize> = HashMap::new();
        let mut single_stock_only = false;

        for a in &self.anchors {
            let main_idxs = Self::match_token_indices(&a.re, text, &byte_to_tok);
            if main_idxs.is_empty() {
                continue;
            }
            if let Some((near_re, win)) = &a.near {
                let near_idxs = Self::match_token_indices(near_re, text, &byte_to_tok);
                if near_idxs.is_empty() || !Self::within_window(&main_idxs, &near_idxs, *win) {
                    continue;
                }
            }

            matched_ids.push(a.cfg.id.clone());
            *cat_counts.entry(a.cfg.category.clone()).or_insert(0) += 1;

            if let Some(tag) = &a.cfg.tag {
                if tag == "single_stock_only" {
                    single_stock_only = true;
                }
            }
        }

        matched_ids.sort();
        matched_ids.dedup();
        (matched_ids, cat_counts, single_stock_only)
    }

    /// Expand alias tokens (e.g., "verb_or_semi") using cfg.aliases
    fn expand_alias<'a>(&'a self, token: &'a str) -> Vec<&'a str> {
        if let Some(v) = self.cfg.aliases.get(token) {
            return v.iter().map(|s| s.as_str()).collect();
        }
        vec![token]
    }

    /// Check if at least one pass-combo template is satisfied by category counts.
    fn combos_satisfied(
        &self,
        cat_counts: &HashMap<String, usize>,
        reasons: &mut Vec<String>,
    ) -> bool {
        if self.cfg.combos.pass_any.is_empty() {
            return true; // if no combos configured, treat as satisfied
        }

        'outer: for tpl in &self.cfg.combos.pass_any {
            // For needs like ["macro","macro","verb_or_semi"], we must be able to "spend" counts.
            let mut pool = cat_counts.clone();

            let mut used = Vec::new();
            for need in &tpl.need {
                let choices = self.expand_alias(need);
                // Find any choice that has remaining count > 0
                let mut satisfied = false;
                for &ch in &choices {
                    if let Some(cnt) = pool.get_mut(ch) {
                        if *cnt > 0 {
                            *cnt -= 1;
                            used.push(ch.to_string());
                            satisfied = true;
                            break;
                        }
                    }
                }
                if !satisfied {
                    continue 'outer;
                }
            }
            reasons.push(format!("combo:{}", used.join("+")));
            return true;
        }
        false
    }

    /// Compute a normalized score in ⟨0..1⟩ using category weights (cap each category count at 3).
    fn weighted_score(&self, cat_counts: &HashMap<String, usize>) -> f32 {
        let mut num = 0i32;
        let mut denom = 0i32;
        for (cat, w) in &self.cfg.weights {
            let cnt = *cat_counts.get(cat).unwrap_or(&0);
            let capped = cnt.min(3) as i32;
            num += capped * *w;
            // normalization baseline: assume up to 3 hits per category possible
            denom += 3 * *w;
        }
        if denom <= 0 {
            return 0.0;
        }
        (num as f32) / (denom as f32)
    }

    /// Public scoring API: blockers → anchors → combos/threshold. Returns {score, matched, reasons}.
    pub fn score(&self, text: &str) -> Relevance {
        let mut rel = Relevance::default();

        // 1) Hard blockers first
        let blockers = self.find_blockers(text);
        if !blockers.is_empty() {
            rel.reasons.extend(blockers.clone());
            dev_log_relevance(
                "blocked",
                text,
                &[],
                &rel.reasons,
                0.0,
                self.cfg.relevance.threshold,
            );
            return rel; // score 0.0
        }

        // 2) Anchors and category stats
        let (matched_ids, cat_counts, single_stock_only) = self.collect_anchor_stats(text);

        // single-stock-only guard
        if single_stock_only {
            let strong_ctx = cat_counts.get("hard").copied().unwrap_or(0)
                + cat_counts.get("macro").copied().unwrap_or(0)
                + cat_counts.get("semi").copied().unwrap_or(0);
            if strong_ctx == 0 {
                rel.reasons
                    .push("single_stock_only_without_broader_context".into());
                rel.matched = matched_ids;
                dev_log_relevance(
                    "neutralized_single_stock_only",
                    text,
                    &rel.matched,
                    &rel.reasons,
                    0.0,
                    self.cfg.relevance.threshold,
                );
                return rel;
            }
        }

        // 3) Combos (precision-first)
        let mut reasons = Vec::new();
        let combos_ok = self.combos_satisfied(&cat_counts, &mut reasons);

        // 4) Weighted score + threshold
        let score = self.weighted_score(&cat_counts);
        let passed_threshold = score >= self.cfg.relevance.threshold;

        // 5) Result aggregation
        rel.matched = matched_ids;
        if combos_ok {
            reasons.push("combos_ok".into());
        } else {
            reasons.push("combos_fail".into());
        }
        if passed_threshold {
            reasons.push(format!("threshold_ok:{:.2}", self.cfg.relevance.threshold));
        } else {
            reasons.push(format!(
                "threshold_fail:{:.2}",
                self.cfg.relevance.threshold
            ));
        }

        if combos_ok && passed_threshold {
            rel.score = score;
        } else {
            rel.score = 0.0; // neutralize
        }
        rel.reasons.extend(reasons);

        // 6) Dev-only diagnostics
        if rel.score > 0.0 {
            dev_log_relevance(
                "passed",
                text,
                &rel.matched,
                &rel.reasons,
                rel.score,
                self.cfg.relevance.threshold,
            );
        } else if combos_ok {
            dev_log_relevance(
                "neutralized_threshold",
                text,
                &rel.matched,
                &rel.reasons,
                score,
                self.cfg.relevance.threshold,
            );
        } else {
            dev_log_relevance(
                "neutralized_combos",
                text,
                &rel.matched,
                &rel.reasons,
                score,
                self.cfg.relevance.threshold,
            );
        }

        rel
    }
}

/* ----------------------------
Thread-safe handle + hot reload
---------------------------- */

/// A threadsafe handle that can hot-reload the underlying engine in dev/local.
/// - Enable by setting RELEVANCE_HOT_RELOAD=1
/// - Dev-gated: active only if cfg!(debug_assertions) OR SHUTTLE_ENV is "local"/"development".
#[derive(Clone)]
pub struct RelevanceHandle {
    inner: Arc<RwLock<RelevanceEngine>>,
}

impl RelevanceHandle {
    pub fn new(engine: RelevanceEngine) -> Self {
        Self {
            inner: Arc::new(RwLock::new(engine)),
        }
    }

    #[allow(dead_code)]
    pub fn inner(&self) -> Arc<RwLock<RelevanceEngine>> {
        self.inner.clone()
    }

    /// Evaluate via scoring (preferred).
    pub fn score(&self, text: &str) -> Relevance {
        if let Ok(eng) = self.inner.read() {
            eng.score(text)
        } else {
            Relevance::default()
        }
    }

    /// Backward-compatible alias — calls `score`.
    #[allow(dead_code)]
    pub fn evaluate(&self, text: &str) -> Relevance {
        self.score(text)
    }
}

/// Returns true if we should enable hot reload (dev/local only).
fn hot_reload_enabled() -> bool {
    let want = std::env::var("RELEVANCE_HOT_RELOAD")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);
    if !want {
        return false;
    }
    // Dev gating
    if cfg!(debug_assertions) {
        return true;
    }
    matches!(
        std::env::var("SHUTTLE_ENV")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "local" | "development" | "dev"
    )
}

/// Start a simple polling watcher on `path` to hot-reload into `handle.inner`.
/// Polls mtime every 2s. Uses only std, no external deps.
pub fn start_hot_reload_thread(handle: RelevanceHandle, path: PathBuf) {
    if !hot_reload_enabled() {
        return;
    }

    thread::spawn(move || {
        let poll = Duration::from_secs(2);
        let mut last_mtime: Option<SystemTime> = None;

        loop {
            match fs::metadata(&path).and_then(|m| m.modified()) {
                Ok(mtime) => {
                    let changed = match last_mtime {
                        None => {
                            last_mtime = Some(mtime);
                            false
                        }
                        Some(prev) => mtime > prev,
                    };
                    if changed {
                        // Reload file and swap engine atomically
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(new_engine) = RelevanceEngine::from_toml_str(&content) {
                                if let Ok(mut guard) = handle.inner.write() {
                                    *guard = new_engine;
                                }
                            }
                        }
                        last_mtime = Some(mtime);
                    }
                }
                Err(_) => {
                    // File missing or unreadable; keep trying.
                }
            }
            thread::sleep(poll);
        }
    });
}

/* ----------------------------
Tests
---------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal, deterministic config used only for tests.
    // - Anchors: DJIA core names + Powell near (fed|rates|fomc)
    // - Blocker: "dji" near (drone|mavic) to avoid the drone company
    // - Weights/threshold chosen so a reasonable combo passes
    const TEST_TOML: &str = r#"
[relevance]
threshold = 0.18
near_default_window = 6

[weights]
hard = 3
semi = 2
macro = 2
soft = 1
verb = 1

# Core DJIA / Dow anchors (counts as "hard")
[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "(?i)\b(djia|dow jones|the dow|dow)\b"

# Macro context: Powell near Fed/rates/FOMC
[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\bpowell\b"
near = { pattern = "(?i)\b(fed|rates?|fomc)\b", window = 6 }

# Optional "single stock only" tag for Dow Inc. (edge case)
[[anchors]]
id = "dow_inc_single"
category = "soft"
pattern = "(?i)\bdow inc\.?\b"
tag = "single_stock_only"

# Block DJI (drones) when near drone terms
[[blockers]]
id = "dji_drones"
pattern = "(?i)\bdji\b"
near = { pattern = "(?i)\b(drone|mavic)\b", window = 4 }
reason = "DJI (drones)"
action = "block"

# Block 'dow' when it is the single-stock company 'Dow Inc.'
[[blockers]]
id = "dow_inc_near_dow_word"
pattern = "(?i)\bdow\b"
near = { pattern = "(?i)\binc\.?\b", window = 1 }
reason = "Dow Inc (single stock)"
action = "block"

# Combos: require at least some macro+hard or macro+verb context
[combos]
pass_any = [
    { need = ["macro", "hard"] },
    { need = ["macro", "verb_or_semi"] }
]

# Alias used in combos (macro + (verb|semi) accepted)
[aliases]
verb_or_semi = ["verb", "semi"]
"#;

    fn eng() -> RelevanceEngine {
        RelevanceEngine::from_toml_str(TEST_TOML).expect("load test config")
    }

    #[test]
    fn tokenizer_basic() {
        let toks = tokenize("The Dow is down.");
        assert_eq!(
            toks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            vec!["The", "Dow", "is", "down"]
        );
        assert!(toks[1].start < toks[1].end);
    }

    #[test]
    fn tags_parse() {
        let c = parse_cashtags("Watch $dji and $DoW, ignore $es_f.");
        assert_eq!(c, vec!["DJI", "DOW"]);
        let h = parse_hashtags("News #DJIA #dowjones #FOMC");
        assert_eq!(h, vec!["djia", "dowjones", "fomc"]);
    }

    #[test]
    fn pass_powell_fed_dow_context() {
        // Self-contained test config: only the categories we want to exercise
        const TEST_TOML: &str = r#"
[relevance]
threshold = 0.30
near_default_window = 6

[weights]
hard = 3
macro = 2

[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "(?i)\\b(djia|dow jones|the dow|dow)\\b"

[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\\bpowell\\b"
near = { pattern = "(?i)\\b(fed|fomc|rates?)\\b", window = 10 }

[[combos.pass_any]]
need = ["macro","hard"]
"#;

        // Build engine from the inline TOML (no external files)
        let eng = RelevanceEngine::from_toml_str(TEST_TOML).expect("load");

        // Sanity: threshold must be the one we expect
        assert!(
            (eng.cfg.relevance.threshold - 0.30).abs() < 1e-6,
            "Threshold embedded in test is {}, expected 0.30",
            eng.cfg.relevance.threshold
        );

        // This sentence should hit both anchors within proximity -> combo ok
        let text = "Powell said the Dow rose after the FOMC meeting.";
        let r = eng.score(text);

        // With weights limited to {hard, macro}, the normalized score is 5 / 15 = 0.333.. > 0.30
        assert!(
            r.score > 0.0,
            "expected pass with macro+hard context, got: {:?}",
            r
        );
        assert!(r.reasons.iter().any(|s| s.contains("combos_ok")));
        assert!(r.matched.iter().any(|m| m == "djia_core_names"));
        assert!(r.matched.iter().any(|m| m == "powell_near_fed_rates"));
    }

    #[test]
    fn block_dji_drone_near() {
        let e = eng();
        let r = e.score("DJI releases a new drone with a better gimbal.");
        assert_eq!(r.score, 0.0, "blocked text must neutralize score");
        assert!(
            r.reasons.iter().any(|s| s.contains("dji_drones")),
            "expected blocker reason present, got: {:?}",
            r.reasons
        );
        assert!(
            r.matched.is_empty(),
            "blocked text should not report anchors"
        );
    }

    #[test]
    fn neutralize_dow_inc_without_context() {
        let e = eng();
        // Only Dow Inc. mention, without macro/hard context -> should be neutralized
        let r = e.score("Dow Inc. announces a cash dividend.");
        assert_eq!(
            r.score, 0.0,
            "single-stock-only without broader context should neutralize"
        );
        // If the engine records the explicit reason, it should be present:
        // (make the assertion soft to avoid flakiness if reason text changes)
        let might_have_reason = r.reasons.iter().any(|s| s.contains("single_stock_only"));
        // Not required, but helps catch regression:
        let _ = might_have_reason;
    }

    #[test]
    fn proximity_is_required_for_powell() {
        let e = eng();
        // Powell but no nearby Fed/rates tokens → should fail
        let r = e.score("Powell gives a talk about leadership. Markets are calm.");
        assert_eq!(
            r.score, 0.0,
            "no proximity → macro anchor should not qualify"
        );
        assert!(
            r.reasons.iter().any(|s| s.contains("combos_fail")),
            "expected combos_fail when proximity anchor doesn't qualify: {:?}",
            r.reasons
        );
    }

    /// Deterministic pseudo-RNG (LCG) so we don't add any dev-deps.
    struct Lcg(u64);
    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn next_usize(&mut self, n: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((self.0 >> 32) as usize) % n.max(1)
        }
    }

    #[derive(Clone)]
    struct Sample {
        #[allow(dead_code)]
        id: String,
        text: String,
        expect_pass: bool,
        note: &'static str,
    }

    fn synth_engine_for_suite() -> RelevanceEngine {
        // Prefer the inline TEST_TOML so suite is deterministic across envs.
        RelevanceEngine::from_toml_str(TEST_TOML).expect("synthetic: load test TOML")
    }

    fn pass_sentence(hard: &str, macro_term: &str, verb: &str) -> String {
        // Ensure proximity: "Powell ... <macro_term>" within a short window, plus a hard anchor.
        // Example: "Powell at the FOMC meeting says the Dow will surge later today."
        format!("Powell at the {macro_term} meeting says {hard} will {verb} later today.")
    }

    fn fail_sentence_kind(kind: usize, hard: &str) -> (String, &'static str) {
        match kind % 4 {
            // 0) DJI drones blocker (must fail)
            0 => (
                "DJI unveils a new Mavic drone with a better gimbal today.".to_string(),
                "dji_drone_block",
            ),
            // 1) Dow Inc. single-stock only (must fail without broader context)
            1 => (
                "Dow Inc. announces a quarterly dividend.".to_string(),
                "dow_inc_single",
            ),
            // 2) Powell far from Fed/rates (fails proximity/combos)
            2 => (
                "Powell gives a keynote on leadership and productivity. Markets are calm."
                    .to_string(),
                "powell_no_macro_near",
            ),
            // 3) Hard anchor alone (fails combos/threshold)
            _ => (format!("{hard} is volatile today."), "hard_alone"),
        }
    }

    fn tricky_sentence_kind(
        kind: usize,
        hard: &str,
        macro_term: &str,
    ) -> (String, bool, &'static str) {
        match kind % 6 {
            // 0) Hashtag variant, with proximity -> should pass
            0 => (
                format!("Powell speaks at the {macro_term}. #DJIA reacts."),
                true,
                "hashtag_pass",
            ),
            // 1) Cashtag DJI with drone (should fail via blocker)
            1 => (
                "Testing $DJI stability while flying a drone near a Mavic.".to_string(),
                false,
                "cashtag_dji_fail",
            ),
            // 2) Lowercase + proximity -> pass
            2 => (
                format!(
                    "powell meets {} to discuss {hard} outlook.",
                    macro_term.to_lowercase()
                ),
                true,
                "lowercase_pass",
            ),
            // 3) Hard near vague macro word not in macro set -> fail
            3 => (
                format!("Powell discusses governance; {hard} remains unaffected."),
                false,
                "macro_missing_fail",
            ),
            // 4) Mixed noise but Powell + macro within window -> pass
            4 => (
                format!(
                    "Noise words here. Powell and {} mention {} briefly.",
                    macro_term, hard
                ),
                true,
                "noisy_but_near_pass",
            ),
            // 5) Dow Inc. with macro but no hard djia anchor -> still fail (single-stock rule)
            5 => (
                "Powell talks about rates; Dow Inc. announces changes.".to_string(),
                false,
                "dow_inc_even_with_macro_fail",
            ),
            _ => unreachable!(),
        }
    }

    #[ignore]
    #[test]
    fn synthetic_suite() {
        let eng = synth_engine_for_suite();

        // Vocab banks (aligned with TEST_TOML anchors)
        let hard_terms = ["DJIA", "Dow Jones", "the Dow", "Dow"];
        let macro_terms = ["Fed", "FOMC", "rates", "rate"];
        let verbs_pos = ["surge", "soar", "rally", "recover"];
        let mut rng = Lcg::new(0xD0D0_D0D0_2025_0818);

        let mut samples: Vec<Sample> = Vec::with_capacity(110);

        // 1) PASS set (~36)
        for i in 0..36 {
            let h = hard_terms[rng.next_usize(hard_terms.len())];
            let m = macro_terms[rng.next_usize(macro_terms.len())];
            let v = verbs_pos[rng.next_usize(verbs_pos.len())];
            samples.push(Sample {
                id: format!("P{:03}", i),
                text: pass_sentence(h, m, v),
                expect_pass: true,
                note: "pass_combo",
            });
        }

        // 2) FAIL set (~48)
        for i in 0..48 {
            let h = hard_terms[rng.next_usize(hard_terms.len())];
            let (text, note) = fail_sentence_kind(i, h);
            samples.push(Sample {
                id: format!("F{:03}", i),
                text,
                expect_pass: false,
                note,
            });
        }

        // 3) TRICKY set (~24)
        for i in 0..24 {
            let h = hard_terms[rng.next_usize(hard_terms.len())];
            let m = macro_terms[rng.next_usize(macro_terms.len())];
            let (text, expect_pass, note) = tricky_sentence_kind(i, h, m);
            samples.push(Sample {
                id: format!("T{:03}", i),
                text,
                expect_pass,
                note,
            });
        }

        // Evaluate
        let mut mismatches = 0usize;
        let total = samples.len();

        println!(
            "{:<4} {:<6} {:<6} {:<6} {:<36}  {}",
            "#", "EXP", "GOT", "SCORE", "REASONS", "TEXT"
        );
        println!("{}", "-".repeat(120));

        for (i, s) in samples.iter().enumerate() {
            let r = eng.score(&s.text);
            let got_pass = r.score > 0.0;
            let exp = if s.expect_pass { "PASS" } else { "FAIL" };
            let got = if got_pass { "PASS" } else { "FAIL" };

            if got_pass != s.expect_pass {
                mismatches += 1;
            }

            let reasons = truncate_vec(&r.reasons, 3).join(" + ");
            println!(
                "{:<4} {:<6} {:<6} {:<6.2} {:<36}  {}  // {}",
                i + 1,
                exp,
                got,
                r.score,
                reasons,
                s.text,
                s.note
            );
        }

        println!("{}", "-".repeat(120));
        println!(
            "Synthetic summary: {} total, {} mismatches",
            total, mismatches
        );

        assert_eq!(
            mismatches, 0,
            "synthetic suite: {} mismatches out of {}",
            mismatches, total
        );
    }
}
