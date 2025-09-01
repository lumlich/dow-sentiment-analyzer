// tests/ai_gate.rs
// Run single-threaded because we mutate process env:
//   cargo test -- --test-threads=1

use std::env;

use dow_sentiment_analyzer::relevance::{
    ai_gate_should_call, extract_threshold_from_reasons, Relevance,
};

fn mk_reasons(threshold: f32) -> Vec<String> {
    vec![format!("threshold_ok:{threshold:.2}")]
}

/// Minimal Relevance for gating tests.
/// Relevance has only {score, matched, reasons}; no `source` field.
fn mk_rel(score: f32, reasons: Vec<String>) -> Relevance {
    Relevance {
        score,
        reasons,
        ..Default::default() // matched stays empty unless needed
    }
}

/// Small RAII helper to snapshot & restore env vars in each test.
struct EnvSnapshot {
    saved: Vec<(String, Option<String>)>,
}
impl EnvSnapshot {
    /// Provide a list of (KEY, Some(VALUE)) to set, or (KEY, None) to remove.
    fn set(pairs: &[(&str, Option<&str>)]) -> Self {
        let mut saved = Vec::with_capacity(pairs.len());
        for (k, v) in pairs {
            let key = k.to_string();
            let prev = env::var(k).ok();
            saved.push((key.clone(), prev));
            match v {
                Some(val) => env::set_var(&key, val),
                None => env::remove_var(&key),
            }
        }
        Self { saved }
    }
}
impl Drop for EnvSnapshot {
    fn drop(&mut self) {
        for (k, maybe_v) in self.saved.drain(..) {
            match maybe_v {
                Some(v) => env::set_var(&k, v),
                None => env::remove_var(&k),
            }
        }
    }
}

/// With AI_TEST_MODE=mock the gate should allow the call regardless of source,
/// as long as we have a positive score and a recognizable threshold tag.
#[test]
fn mock_bypass_allows_call() {
    let _env = EnvSnapshot::set(&[
        ("AI_TEST_MODE", Some("mock")),
        ("AI_ENABLED", Some("1")),
        ("AI_ONLY_TOP_SOURCES", Some("1")),
        ("AI_SOURCES", None),
        ("AI_SCORE_BAND", None),
    ]);

    let rel = mk_rel(0.64, mk_reasons(0.50));
    let call = ai_gate_should_call("totally-unknown-source", &rel);
    assert!(
        call,
        "mock mode should bypass source allowlist and band checks"
    );
}

/// When top-sources filter is ON and the source is not whitelisted,
/// the gate should block the call.
#[test]
fn top_sources_filter_blocks_unknown() {
    let _env = EnvSnapshot::set(&[
        ("AI_TEST_MODE", Some("off")),
        ("AI_ENABLED", Some("1")),
        ("AI_ONLY_TOP_SOURCES", Some("1")),
        ("AI_SOURCES", None), // use built-in allowlist only
        ("AI_SCORE_BAND", Some("0.08")),
    ]);

    let rel = mk_rel(0.51, mk_reasons(0.50));
    let call = ai_gate_should_call("not-whitelisted-source", &rel);
    assert!(
        !call,
        "unknown source should be blocked when AI_ONLY_TOP_SOURCES=1"
    );
}

/// Custom allowlist via AI_SOURCES should allow a matching source even if the built-in list
/// would not include it.
#[test]
fn custom_allowlist_allows_match() {
    let _env = EnvSnapshot::set(&[
        ("AI_TEST_MODE", Some("off")),
        ("AI_ENABLED", Some("1")),
        ("AI_ONLY_TOP_SOURCES", Some("1")),
        ("AI_SOURCES", Some("Foo,Bar")),
        ("AI_SCORE_BAND", Some("0.08")),
    ]);

    let rel = mk_rel(0.51, mk_reasons(0.50));
    let call = ai_gate_should_call("Foo", &rel);
    assert!(
        call,
        "source present in AI_SOURCES should pass the allowlist when filter is ON"
    );
}

/// The near-threshold band controls whether we call AI:
/// - If (score - threshold) > band  → do NOT call
/// - If (score - threshold) <= band → call
#[test]
fn band_limits_calls() {
    let _env = EnvSnapshot::set(&[
        ("AI_TEST_MODE", Some("off")),
        ("AI_ENABLED", Some("1")),
        ("AI_ONLY_TOP_SOURCES", Some("0")), // disable source filtering to isolate band behavior
        ("AI_SOURCES", None),
        ("AI_SCORE_BAND", Some("0.02")),
    ]);

    // Outside the band: 0.53 - 0.50 = 0.03 > 0.02 → false
    let rel_outside = mk_rel(0.53, mk_reasons(0.50));
    let call_outside = ai_gate_should_call("any", &rel_outside);
    assert!(
        !call_outside,
        "if diff > band, gate should NOT call the AI (expected false)"
    );

    // Inside the band: 0.51 - 0.50 = 0.01 <= 0.02 → true
    let rel_inside = mk_rel(0.51, mk_reasons(0.50));
    let call_inside = ai_gate_should_call("any", &rel_inside);
    assert!(
        call_inside,
        "if diff <= band, gate SHOULD call the AI (expected true)"
    );
}

/// A quick sanity check for the threshold extraction utility.
#[test]
fn extracts_threshold_from_relevance() {
    let rel = Relevance {
        score: 0.72,
        reasons: vec![
            "some note".to_string(),
            "threshold_ok:0.72".to_string(),
            "other tag".to_string(),
        ],
        ..Default::default()
    };
    let th = extract_threshold_from_reasons(&rel).expect("should parse the threshold from reasons");
    assert!((th - 0.72).abs() < 1e-6);
}
