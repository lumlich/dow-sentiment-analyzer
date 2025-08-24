// tests/relevance_handpicked.rs
// Hand-picked unit tests for the relevance gate.
// These tests are self-contained: they use an inline TOML config.
// Keep code and comments in English.

use dow_sentiment_analyzer::relevance::RelevanceEngine;

const TEST_TOML: &str = r#"
[relevance]
threshold = 0.18
near_default_window = 6

[weights]
hard = 3
macro = 2
semi = 2
soft = 1
verb = 1

# Core DJIA / Dow anchors (counts as "hard")
[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "(?i)\\b(djia|dow jones|the dow|dow)\\b"

# Macro context: Powell near (Fed|rates|FOMC)
[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\\bpowell\\b"
near = { pattern = "(?i)\\b(fed|fomc|rates?)\\b", window = 6 }

# Optional single-stock guard for Dow Inc.
[[anchors]]
id = "dow_inc_single"
category = "soft"
pattern = "(?i)\\bdow inc\\.?\\b"
tag = "single_stock_only"

# Block DJI (drones) when close to drone terms
[[blockers]]
id = "dji_drones"
pattern = "(?i)\\bdji\\b"
near = { pattern = "(?i)\\b(drone|mavic)\\b", window = 4 }
reason = "DJI (drones)"
action = "block"

# Block 'dow' when it's the single-stock company 'Dow Inc.'
[[blockers]]
id = "dow_inc_near_dow_word"
pattern = "(?i)\\bdow\\b"
near = { pattern = "(?i)\\binc\\.?\\b", window = 1 }
reason = "Dow Inc (single stock)"
action = "block"

# Require either (macro + hard) or (macro + verb_or_semi)
[combos]
pass_any = [
    { need = ["macro", "hard"] },
    { need = ["macro", "verb_or_semi"] }
]

[aliases]
verb_or_semi = ["verb", "semi"]
"#;

fn eng() -> RelevanceEngine {
    RelevanceEngine::from_toml_str(TEST_TOML).expect("load inline test config")
}

#[test]
fn pass_powell_fed_dow_context() {
    let e = eng();
    let text = "Fed chair Powell comments on the Dow after the FOMC meeting.";
    let r = e.score(text);
    assert!(
        r.score > 0.0,
        "expected PASS with macro+hard context, got: {:?}",
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
        "expected blocker reason"
    );
    assert!(
        r.matched.is_empty(),
        "blocked text should not report anchors"
    );
}

#[test]
fn neutralize_dow_inc_without_context() {
    let e = eng();
    let r = e.score("Dow Inc. announces a cash dividend.");
    assert_eq!(
        r.score, 0.0,
        "single-stock-only without broader context should neutralize"
    );
    // soft check of reason (optional wording):
    let _maybe_reason = r.reasons.iter().any(|s| s.contains("single_stock_only"));
}

#[test]
fn proximity_required_for_powell() {
    let e = eng();
    let r = e.score("Powell gives a talk about leadership. Markets are calm.");
    assert_eq!(
        r.score, 0.0,
        "no proximity → macro anchor should not qualify"
    );
    assert!(
        r.reasons.iter().any(|s| s.contains("combos_fail")),
        "expected combos_fail"
    );
}
