// src/ingest/mod.rs
pub mod backup;
pub mod config;
pub mod providers;
pub mod scheduler;
pub mod types;

use crate::ingest::types::{SourceEvent, SourceProvider};
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge};
use once_cell::sync::OnceCell;
use std::collections::HashSet;

/// One-time metrics registration (so series show up on /metrics).
fn ensure_metrics_described() {
    static ONCE: OnceCell<()> = OnceCell::new();
    ONCE.get_or_init(|| {
        describe_counter!("ingest_events_total", "Total events parsed from providers.");
        describe_counter!(
            "ingest_kept_total",
            "Events kept after normalization + filtering."
        );
        describe_counter!(
            "ingest_filtered_total",
            "Events filtered out due to whitelist/empty."
        );
        describe_counter!(
            "ingest_dedup_total",
            "Events removed by deduplication window."
        );
        describe_counter!(
            "ingest_provider_errors_total",
            "Provider fetch/parse errors."
        );
        describe_histogram!("ingest_parse_ms", "Provider parse time in milliseconds.");
        describe_gauge!(
            "ingest_pipeline_last_run_ts",
            "Unix ts when ingest pipeline last ran."
        );
    });
}

/// Normalize text: collapse whitespace, trim, strip stray punctuation.
pub fn normalize_text(s: &str) -> String {
    // 1) HTML entity decode
    let mut out = html_escape::decode_html_entities(s).to_string();

    // 2) Strip HTML tags
    static RE_TAGS: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
    let re_tags = RE_TAGS.get_or_init(|| regex::Regex::new(r"(?is)</?[^>]+>").unwrap());
    out = re_tags.replace_all(&out, "").to_string();

    // 3) Normalize “ ” ‘ ’ « » to ASCII quotes
    out = out
        .replace(['\u{201C}', '\u{201D}', '\u{00AB}', '\u{00BB}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'");

    // 4) Collapse whitespace
    static RE_WS: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
    let re_ws = RE_WS.get_or_init(|| regex::Regex::new(r"\s+").unwrap());
    out = re_ws.replace_all(&out, " ").to_string();
    out = out.trim().to_string();

    // 5) Strip trailing sentence punctuation (keep quotes)
    while let Some(last) = out.chars().last() {
        if matches!(last, '!' | '?' | '.' | ',') {
            out.pop();
        } else {
            break;
        }
    }

    // 6) Length cap: 1500 chars
    if out.chars().count() > 1500 {
        out = out.chars().take(1500).collect();
    }

    out
}

pub fn is_whitelisted<S: AsRef<str>>(source: S, whitelist: &[String]) -> bool {
    let s = source.as_ref();
    whitelist.iter().any(|w| w.eq_ignore_ascii_case(s))
}

pub fn normalize_filter_dedup(
    now: u64,
    raw_events: Vec<SourceEvent>,
    whitelist: &[String],
    dedup_window_secs: u64,
) -> (Vec<SourceEvent>, usize, usize) {
    // Normalize + filter
    let mut filtered_out = 0usize;
    let mut filtered = Vec::with_capacity(raw_events.len());
    for mut ev in raw_events {
        ev.text = normalize_text(&ev.text);
        let keep =
            !ev.text.is_empty() && (whitelist.is_empty() || is_whitelisted(&ev.source, whitelist));
        if !keep {
            filtered_out += 1;
            continue;
        }
        filtered.push(ev);
    }

    // Deduplicate by text for recent items only (within window).
    let mut seen_texts: HashSet<String> = HashSet::new();
    let mut keep = Vec::with_capacity(filtered.len());
    let mut dedup_out = 0usize;

    for ev in filtered.into_iter() {
        let is_recent = now.saturating_sub(ev.published_at) <= dedup_window_secs;
        if is_recent && !seen_texts.insert(ev.text.clone()) {
            dedup_out += 1;
            continue;
        }
        keep.push(ev);
    }

    (keep, filtered_out, dedup_out)
}

/// Run ingest once using the provided providers and configuration.
/// Returns (kept, filtered_count, dedup_count).
pub async fn run_once(
    providers: &[Box<dyn SourceProvider>],
    whitelist: &[String],
    dedup_window_secs: u64,
) -> (Vec<SourceEvent>, usize, usize) {
    ensure_metrics_described();

    let mut raw = Vec::new();
    for p in providers {
        match p.fetch_latest().await {
            Ok(mut v) => raw.append(&mut v),
            Err(e) => {
                tracing::warn!(error = ?e, provider = p.name(), "provider error");
                counter!("ingest_provider_errors_total").increment(1);
            }
        }
    }

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (kept, filtered_cnt, dedup_cnt) =
        normalize_filter_dedup(now, raw, whitelist, dedup_window_secs);

    // Telemetry
    counter!("ingest_kept_total").increment(kept.len() as u64);
    counter!("ingest_filtered_total").increment(filtered_cnt as u64);
    counter!("ingest_dedup_total").increment(dedup_cnt as u64);
    gauge!("ingest_pipeline_last_run_ts").set(now as f64);

    (kept, filtered_cnt, dedup_cnt)
}

/// Backward-compatible helper with empty whitelist and default 600s window.
/// Keeps existing tests working.
pub async fn run_once_with_empty_whitelist(
    providers: &[Box<dyn SourceProvider>],
) -> Vec<SourceEvent> {
    run_once(providers, &[], 600).await.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_text_collapses_ws_and_punct() {
        let s = "  Hello,&nbsp;&nbsp; world!!!  ";
        let out = normalize_text(s);
        assert_eq!(out, "Hello, world");
    }

    #[test]
    fn whitelist_matching_is_case_insensitive() {
        let wl = vec!["Fed".to_string(), "Reuters".into()];
        assert!(is_whitelisted("fed", &wl));
        assert!(is_whitelisted("REUTERS", &wl));
        assert!(!is_whitelisted("Bloomberg", &wl));
    }

    #[test]
    fn dedup_by_text_within_window() {
        let now = 1000u64;
        let wl: Vec<String> = vec![];
        let evs = vec![
            SourceEvent {
                source: "Fed".into(),
                published_at: 995,
                text: "abc".into(),
                url: None,
                priority_hint: None,
            },
            SourceEvent {
                source: "Fed".into(),
                published_at: 996,
                text: "abc".into(),
                url: None,
                priority_hint: None,
            },
            SourceEvent {
                source: "Fed".into(),
                published_at: 300,
                text: "abc".into(),
                url: None,
                priority_hint: None,
            },
        ];
        let (kept, filtered, dedup) = normalize_filter_dedup(now, evs, &wl, 600);
        assert_eq!(kept.len(), 2); // one deduped within window; the old one kept
        assert_eq!(filtered, 0);
        assert_eq!(dedup, 1);
    }
}
