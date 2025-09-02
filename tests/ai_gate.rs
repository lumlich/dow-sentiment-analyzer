// tests/ai_gate.rs
// Run single-threaded if you mutate process env in other tests:
//   cargo test -- --test-threads=1

use dow_sentiment_analyzer::relevance::{extract_threshold_from_reasons, Relevance};

/// Helper to build a minimal Relevance with a threshold tag inside reasons.
/// The project-side parser expects a token like `threshold_ok:<num>`.
fn mk_relevance_with_threshold(score: f32, threshold: f32) -> Relevance {
    Relevance {
        score,
        reasons: vec![
            "note: macro/hard combo".to_string(),
            format!("threshold_ok:{threshold:.2}"),
            "other tag".to_string(),
        ],
        ..Default::default()
    }
}

#[test]
fn extracts_threshold_from_relevance() {
    let rel = mk_relevance_with_threshold(0.72, 0.72);
    let th = extract_threshold_from_reasons(&rel).expect("should parse threshold from reasons");
    assert!((th - 0.72).abs() < 1e-6, "unexpected threshold value: {th}");
}

#[test]
fn returns_none_when_no_threshold_tag_present() {
    let rel = Relevance {
        score: 0.31,
        reasons: vec!["some note".into(), "no threshold here".into()],
        ..Default::default()
    };
    let th = extract_threshold_from_reasons(&rel);
    assert!(th.is_none(), "expected None when threshold tag is absent");
}
