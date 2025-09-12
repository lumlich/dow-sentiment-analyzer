//! AI gating boundary & extraction tests adapted to current signatures:
//!   - extract_threshold_from_reasons(&Relevance) -> Option<f32>
//!   - ai_gate_should_call(&str, &Relevance) -> bool
//! Uses Relevance::test_new(...) helper defined in src/relevance.rs.

use dow_sentiment_analyzer::relevance::{ai_gate_should_call, extract_threshold_from_reasons};
use dow_sentiment_analyzer::Relevance; // re-export z lib.rs

const DEFAULT_THRESHOLD: f32 = 0.50; // jen pro výpočty v testu
const DEFAULT_BAND: f32 = 0.08; // default, pokud AI_SCORE_BAND není v env
const TOP_SOURCE: &str = "Reuters"; // v default allowlistu

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Vytvoř Relevance s daným score a případným threshold tagem.
/// `threshold` se do Relevance dostane přes důvod "threshold_ok:<num>".
fn rel(score: f32, threshold: Option<f32>, extra_reasons: &[&str]) -> Relevance {
    let mut reasons = s(extra_reasons);
    if let Some(t) = threshold {
        reasons.push(format!("threshold_ok:{t:.2}"));
    }
    // Relevance::test_new: (score, _threshold, _band, reasons, _label)
    Relevance::test_new(score, 0.0, 0.0, reasons, "unit")
}

/* ------------------------------
   Threshold extraction tests
--------------------------------*/

#[test]
fn threshold_extraction_simple_ok_tag() {
    let r = rel(0.0, None, &["threshold_ok:0.72"]);
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!((t - 0.72).abs() < 1e-6, "expected 0.72, got {t}");
}

#[test]
fn threshold_extraction_ignores_other_text() {
    let r = rel(0.0, None, &["Some note", "threshold_ok:0.35", "other"]);
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!((t - 0.35).abs() < 1e-6, "expected 0.35, got {t}");
}

#[test]
fn threshold_extraction_first_valid_wins_when_multiple_present() {
    // Implementace prochází reasons v pořadí a vrátí první validní výskyt
    let r = rel(
        0.0,
        None,
        &["threshold_ok:0.61", "note", "threshold_ok:0.42"],
    );
    let t = extract_threshold_from_reasons(&r).expect("threshold expected");
    assert!(
        (t - 0.61).abs() < 1e-6,
        "expected first valid 0.61, got {t}"
    );
}

#[test]
fn threshold_extraction_missing_returns_none() {
    let r = rel(0.0, None, &["no threshold here", "band=0.08"]);
    let t = extract_threshold_from_reasons(&r);
    assert!(
        t.is_none(),
        "expected None when no threshold_ok present, got {t:?}"
    );
}

/* ------------------------------
   AI gate band tests (default band = 0.08)
   Pozn.: gate volá jen když rel.score > 0 a zdroj je v allowlistu.
--------------------------------*/

#[test]
fn gate_calls_inside_band_above_threshold() {
    // přesně na threshold → diff = 0.0 → uvnitř pásma
    let r = rel(DEFAULT_THRESHOLD, Some(DEFAULT_THRESHOLD), &[]);
    assert!(
        ai_gate_should_call(TOP_SOURCE, &r),
        "call at exact threshold"
    );

    // těsně uvnitř horní hrany
    let r = rel(DEFAULT_THRESHOLD + 0.0799, Some(DEFAULT_THRESHOLD), &[]);
    assert!(
        ai_gate_should_call(TOP_SOURCE, &r),
        "call just inside upper edge"
    );
}

#[test]
fn gate_calls_even_when_just_below_threshold_if_score_positive() {
    // pokud je score kladné, diff = max(score - thr, 0) → 0.0 ⇒ uvnitř pásma
    let r = rel(DEFAULT_THRESHOLD - 0.01, Some(DEFAULT_THRESHOLD), &[]);
    assert!(
        ai_gate_should_call(TOP_SOURCE, &r),
        "positive score just below threshold should still call"
    );
}

#[test]
fn gate_does_not_call_outside_band_above_threshold() {
    let r = rel(
        DEFAULT_THRESHOLD + DEFAULT_BAND + 0.0002,
        Some(DEFAULT_THRESHOLD),
        &[],
    );
    assert!(
        !ai_gate_should_call(TOP_SOURCE, &r),
        "NO call just outside upper edge"
    );
}

#[test]
fn gate_is_inclusive_on_upper_band_edge() {
    let r = rel(
        DEFAULT_THRESHOLD + DEFAULT_BAND,
        Some(DEFAULT_THRESHOLD),
        &[],
    );
    assert!(
        ai_gate_should_call(TOP_SOURCE, &r),
        "call at inclusive upper edge"
    );
}

#[test]
fn gate_uses_extracted_threshold_when_available() {
    // reasons nesou threshold_ok:0.62
    let r = rel(0.70, None, &["threshold_ok:0.62"]);
    let thr = extract_threshold_from_reasons(&r).expect("extraction failed");
    assert!((thr - 0.62).abs() < 1e-6, "extraction failed, got {thr}");

    // horní hrana = 0.62 + 0.08 = 0.70 → inclusive => call
    assert!(
        ai_gate_should_call(TOP_SOURCE, &r),
        "expected call at upper edge with extracted threshold"
    );

    // mírně nad hranou → NO call
    let r2 = rel(0.7002, None, &["threshold_ok:0.62"]);
    assert!(
        !ai_gate_should_call(TOP_SOURCE, &r2),
        "expected NO call just above edge"
    );
}

#[test]
fn gate_with_weird_scores_outside_band_or_neutralized() {
    // zero nebo záporné skóre → nikdy nevolá
    let r = rel(0.0, Some(DEFAULT_THRESHOLD), &[]);
    assert!(!ai_gate_should_call(TOP_SOURCE, &r));

    let r = rel(-10.0_f32, Some(DEFAULT_THRESHOLD), &[]);
    assert!(!ai_gate_should_call(TOP_SOURCE, &r));
}
