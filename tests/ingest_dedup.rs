// tests/ingest_dedup.rs
use dow_sentiment_analyzer::ingest::normalize_filter_dedup;
use dow_sentiment_analyzer::ingest::types::SourceEvent;

#[test]
fn repeated_texts_in_window_are_ignored() {
    let now = 2_000_000;
    let wl = vec!["Fed".to_string(), "Reuters".to_string()];
    let txt = "Same sentence";

    let raw = vec![
        SourceEvent {
            source: "Fed".into(),
            published_at: now - 100,
            text: txt.into(),
            url: None,
            priority_hint: None,
        },
        SourceEvent {
            source: "Reuters".into(),
            published_at: now - 90,
            text: txt.into(),
            url: None,
            priority_hint: None,
        },
        SourceEvent {
            source: "Fed".into(),
            published_at: now - 10_000,
            text: txt.into(),
            url: None,
            priority_hint: None,
        }, // outside window
    ];

    let (kept, _filtered, dedup) = normalize_filter_dedup(now, raw, &wl, 600);
    // should keep first recent + old one, drop the second recent duplicate
    assert_eq!(kept.len(), 2);
    assert_eq!(dedup, 1);
}
