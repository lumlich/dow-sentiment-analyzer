//! Contextual rules engine (hot-reloaded from `config/rules.json`).
//!
//! Minimal JSON DSL for conditions over the input text (case-insensitive):
//! - `any_contains`: match if ANY of phrases appears
//! - `all_contains`: match if ALL of phrases appear
//! - `not_contains`: match if NONE of phrases appear
//! - `min_len`:      match if input length >= min_len (chars)
//!
//! Actions when a rule matches:
//! - `set_action`:        "BUY" | "SELL" | "HOLD" | custom
//! - `boost_confidence`:  f32 delta added to confidence (clamped later to [0,1])
//! - `add_reason`:        string appended to reasons
//!
//! The file is hot-reloaded on mtime change at each `apply_rules()` call.

use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::RwLock,
    time::SystemTime,
};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub name: Option<String>,
    #[serde(default)]
    pub when: When,
    #[serde(default)]
    pub then: Then,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct When {
    pub any_contains: Option<Vec<String>>,
    pub all_contains: Option<Vec<String>>,
    pub not_contains: Option<Vec<String>>,
    pub min_len: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Then {
    pub set_action: Option<String>,
    pub boost_confidence: Option<f32>,
    pub add_reason: Option<String>,
}

#[derive(Debug)]
pub struct HotReloadRules {
    path: PathBuf,
    inner: RwLock<State>,
}

#[derive(Debug)]
struct State {
    rules: RuleSet,
    last_modified: Option<SystemTime>,
}

impl HotReloadRules {
    pub fn new(path: Option<&Path>) -> Self {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("config/rules.json"));
        Self {
            path,
            inner: RwLock::new(State {
                rules: RuleSet::default(),
                last_modified: None,
            }),
        }
    }

    pub fn current(&self) -> RuleSet {
        // Check if reload is needed
        let (needs_reload, _new_mtime) = match fs::metadata(&self.path).and_then(|m| m.modified()) {
            Ok(mtime) => {
                let guard = self.inner.read().unwrap();
                let changed = guard.last_modified != Some(mtime);
                (changed, Some(mtime))
            }
            Err(_) => (false, None),
        };

        if !needs_reload {
            return self.inner.read().unwrap().rules.clone();
        }

        let mut guard = self.inner.write().unwrap();
        if let Ok(meta) = fs::metadata(&self.path) {
            if let Ok(mtime) = meta.modified() {
                if guard.last_modified != Some(mtime) {
                    if let Ok(rules) = load_rules_file(&self.path) {
                        guard.rules = rules;
                        guard.last_modified = Some(mtime);
                    }
                }
            }
        }
        guard.rules.clone()
    }
}

pub fn load_rules_file(path: &Path) -> io::Result<RuleSet> {
    let bytes = fs::read(path)?;
    let rules: RuleSet = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(rules)
}

/// Apply rules to `(action, confidence, reasons)` given the `input_text`.
/// Returns possibly updated `(action, confidence_delta, appended_reasons)`.
pub fn apply_rules_to_text(
    input_text: &str,
    rules: &RuleSet,
) -> (Option<String>, f32, Vec<String>) {
    let text = normalize(input_text);

    let mut new_action: Option<String> = None;
    let mut delta_conf: f32 = 0.0;
    let mut extra_reasons: Vec<String> = Vec::new();

    for rule in &rules.rules {
        if matches_when(&text, &rule.when) {
            if let Some(a) = &rule.then.set_action {
                // Last matching rule wins for action (simple precedence).
                new_action = Some(a.clone());
            }
            if let Some(d) = rule.then.boost_confidence {
                delta_conf += d;
            }
            if let Some(r) = &rule.then.add_reason {
                extra_reasons.push(r.clone());
            }
        }
    }

    (new_action, delta_conf, extra_reasons)
}

// --- internals ---

fn matches_when(text: &str, w: &When) -> bool {
    if let Some(min) = w.min_len {
        if text.chars().count() < min {
            return false;
        }
    }
    if let Some(v) = &w.any_contains {
        if !v.iter().any(|p| contains(text, p)) {
            return false;
        }
    }
    if let Some(v) = &w.all_contains {
        if !v.iter().all(|p| contains(text, p)) {
            return false;
        }
    }
    if let Some(v) = &w.not_contains {
        if v.iter().any(|p| contains(text, p)) {
            return false;
        }
    }
    true
}

fn contains(text: &str, pat: &str) -> bool {
    // Normalize both sides (lowercase + condensed spaces),
    // then plain `contains(&str)`.
    let t = normalize(text);
    let p = normalize(pat);
    if p.is_empty() {
        return true;
    }
    t.contains(p.as_str())
}

fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = false;
    for ch in input.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(lc);
            last_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_rule_hits() {
        let rules = RuleSet {
            rules: vec![Rule {
                name: Some("buy on cut".into()),
                when: When {
                    any_contains: Some(vec!["rate cut".into(), "cuts rates".into()]),
                    all_contains: None,
                    not_contains: None,
                    min_len: None,
                },
                then: Then {
                    set_action: Some("BUY".into()),
                    boost_confidence: Some(0.2),
                    add_reason: Some("Matched rule: policy easing".into()),
                },
            }],
        };

        let (a, d, extra) = apply_rules_to_text("Breaking: Fed cuts rates today", &rules);
        assert_eq!(a.as_deref(), Some("BUY"));
        assert!((d - 0.2).abs() < 1e-6);
        assert_eq!(extra.len(), 1);
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        let rules = RuleSet {
            rules: vec![Rule {
                name: Some("lenient contains".into()),
                when: When {
                    any_contains: Some(vec!["policy easing".into()]),
                    ..Default::default()
                },
                then: Then {
                    add_reason: Some("found".into()),
                    ..Default::default()
                },
            }],
        };
        let (a, d, extra) = apply_rules_to_text("  POLICY   EASING\tconfirmed ", &rules);
        assert!(a.is_none());
        assert_eq!(d, 0.0);
        assert_eq!(extra, vec!["found"]);
    }
}
