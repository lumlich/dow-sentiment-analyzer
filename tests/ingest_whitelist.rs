// tests/ingest_whitelist.rs
use dow_sentiment_analyzer::ingest::types::SourceEvent;
use dow_sentiment_analyzer::ingest::{is_whitelisted, normalize_filter_dedup};

#[test]
fn non_whitelisted_is_filtered() {
    let wl = vec!["Fed".to_string()];
    let now = 1_000_000;

    let raw = vec![
        SourceEvent {
            source: "Fed".into(),
            published_at: now,
            text: "ok".into(),
            url: None,
            priority_hint: None,
        },
        SourceEvent {
            source: "RandomBlog".into(),
            published_at: now,
            text: "nope".into(),
            url: None,
            priority_hint: None,
        },
    ];

    let (kept, filtered, _dedup) = normalize_filter_dedup(now, raw, &wl, 600);
    assert_eq!(kept.len(), 1);
    assert_eq!(filtered, 1);
    assert!(is_whitelisted("Fed", &wl));
    assert!(!is_whitelisted("RandomBlog", &wl));
}
