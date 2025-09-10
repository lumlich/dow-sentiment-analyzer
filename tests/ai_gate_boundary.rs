//! AI gating boundary & extraction tests adapted to current signatures:
//!   - extract_threshold_from_reasons(&Relevance) -> Option<f32>
//!   - ai_gate_should_call(&str, &Relevance) -> bool
//! Uses Relevance::test_new(...) helper (cfg(test)) defined in src/relevance.rs.

use dow_sentiment_analyzer::relevance::{
    ai_gate_should_call, extract_threshold_from_reasons, Relevance,
};

const DEFAULT_THRESHOLD: f32 = 0.50;
const DEFAULT_BAND: f32 = 0.08;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn rel(score: f32, threshold: Option<f32>, band: f32, reasons: &[&str]) -> Relevance {
    Relevance::test_new(score, threshold, band, s(reasons), "unit")
}

#[test]
fn threshold_extraction_simple_equals() {
    let r = rel(0.0, None, DEFAULT_BAND, &["threshold=0.72"]);
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!((t - 0.72).abs() < 1e-6, "expected 0.72, got {t}");
}

#[test]
fn threshold_extraction_with_colon_and_spaces_case_insensitive() {
    let r = rel(
        0.0,
        None,
        DEFAULT_BAND,
        &["Some note", "ThReShOlD : 0.35", "other"],
    );
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!((t - 0.35).abs() < 1e-6, "expected 0.35, got {t}");
}

#[test]
fn threshold_extraction_ignores_invalid_and_out_of_range() {
    let r = rel(
        0.0,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &["threshold=abc", "threshold=1.42", "threshold=-0.1"],
    );
    let t = extract_threshold_from_reasons(&r).unwrap_or(DEFAULT_THRESHOLD);
    assert!((t - DEFAULT_THRESHOLD).abs() < 1e-6);
}

#[test]
fn threshold_extraction_first_valid_wins_when_multiple_present() {
    // If your impl chooses "first", this asserts 0.61; if you choose "last"/"max",
    // adjust test or impl accordingly.
    let r = rel(
        0.0,
        None,
        DEFAULT_BAND,
        &["threshold=0.61", "note", "threshold=0.42"],
    );
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!(
        (t - 0.61).abs() < 1e-6,
        "expected first valid 0.61, got {t}"
    );
}

#[test]
fn threshold_extraction_missing_returns_none() {
    let r = rel(
        0.0,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &["no threshold here", "band=0.08"],
    );
    let t = extract_threshold_from_reasons(&r);
    assert!(
        t.is_none(),
        "expected None when no threshold present, got {t:?}"
    );
}

#[test]
fn gate_calls_inside_band_centered_on_default_threshold() {
    // Build rel around the default threshold/band and vary only the score.
    // Exactly at threshold → should call (inclusive).
    let r = rel(
        DEFAULT_THRESHOLD,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(ai_gate_should_call("unit", &r), "call at exact threshold");

    // Slightly inside upper edge
    let r = rel(
        DEFAULT_THRESHOLD + 0.0799,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        ai_gate_should_call("unit", &r),
        "call just inside upper edge"
    );

    // Slightly inside lower edge
    let r = rel(
        DEFAULT_THRESHOLD - 0.0799,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        ai_gate_should_call("unit", &r),
        "call just inside lower edge"
    );
}

#[test]
fn gate_does_not_call_outside_band() {
    let r = rel(
        DEFAULT_THRESHOLD + 0.0801,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        !ai_gate_should_call("unit", &r),
        "NO call just outside upper edge"
    );

    let r = rel(
        DEFAULT_THRESHOLD - 0.0801,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        !ai_gate_should_call("unit", &r),
        "NO call just outside lower edge"
    );
}

#[test]
fn gate_is_inclusive_on_band_edges() {
    let r = rel(
        DEFAULT_THRESHOLD + DEFAULT_BAND,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        ai_gate_should_call("unit", &r),
        "call at inclusive upper edge"
    );

    let r = rel(
        DEFAULT_THRESHOLD - DEFAULT_BAND,
        Some(DEFAULT_THRESHOLD),
        DEFAULT_BAND,
        &[],
    );
    assert!(
        ai_gate_should_call("unit", &r),
        "call at inclusive lower edge"
    );
}

#[test]
fn gate_uses_extracted_threshold_when_available() {
    // reasons request threshold=0.62; band stays default in rel
    let r = rel(0.70, None, DEFAULT_BAND, &["threshold=0.62"]);
    let thr = extract_threshold_from_reasons(&r).expect("extraction failed");
    assert!((thr - 0.62).abs() < 1e-6, "extraction failed, got {thr}");

    // Rebuild rel to reflect the extracted threshold for gating check:
    let r2 = rel(0.70, Some(thr), DEFAULT_BAND, &["threshold=0.62"]);
    assert!(
        ai_gate_should_call("unit", &r2),
        "expected call at upper edge"
    );

    let r3 = rel(0.7002, Some(thr), DEFAULT_BAND, &["threshold=0.62"]);
    assert!(
        !ai_gate_should_call("unit", &r3),
        "expected NO call just above edge"
    );
}

#[test]
fn gate_with_weird_scores_outside_band() {
    // Ensure no panic and returns false for absurd scores.
    let r = rel(-10.0, Some(DEFAULT_THRESHOLD), DEFAULT_BAND, &[]);
    assert!(!ai_gate_should_call("unit", &r));

    let r = rel(10.0, Some(DEFAULT_THRESHOLD), DEFAULT_BAND, &[]);
    assert!(!ai_gate_should_call("unit", &r));
}
