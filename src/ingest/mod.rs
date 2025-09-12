// src/ingest/mod.rs
pub mod backup;
pub mod providers;
pub mod types;

use crate::ingest::types::SourceEvent;
use html_escape;
use metrics::{counter, histogram};
use once_cell::sync::Lazy;
use regex::Regex;

/// Normalize input text: strip HTML, unescape entities, fold whitespace, map typographic quotes to ASCII,
/// collapse NBSP, trim, and length-cap.
pub fn normalize_text(s: &str) -> String {
    // 1) Remove HTML tags (coarse but deterministic).
    //    We intentionally do not keep inline <br> as newlines to keep a compact signal for rules.
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());

    // 2) HTML entities decode.
    let without_tags = TAG_RE.replace_all(s, "");
    let unescaped = html_escape::decode_html_entities(&without_tags).to_string();

    // 3) Map common typographic quotes/dashes to ASCII equivalents.
    let mapped = unescaped
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(['\u{2013}', '\u{2014}'], "-"); // NBSP -> space

    // 4) Fold whitespace.
    static WS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
    let folded = WS_RE.replace_all(mapped.trim(), " ");

    // 5) Length cap (safety net).
    const MAX_LEN: usize = 1_500;
    let mut out = folded.to_string();
    if out.len() > MAX_LEN {
        out.truncate(MAX_LEN);
    }
    out
}

/// Return true if source is in authority whitelist.
pub fn is_whitelisted<S: AsRef<str>>(source: S, whitelist: &[String]) -> bool {
    let s = source.as_ref();
    whitelist.iter().any(|w| w.eq_ignore_ascii_case(s))
}

/// Ingest pipeline: normalize -> filter (whitelist) -> dedup.
/// Returns (kept_events, filtered_count, dedup_count).
pub fn normalize_filter_dedup(
    now: u64,
    raw_events: Vec<SourceEvent>,
    whitelist: &[String],
    dedup_window_secs: u64,
) -> (Vec<SourceEvent>, usize, usize) {
    let mut filtered = Vec::with_capacity(raw_events.len());
    let mut filtered_out = 0usize;
    let mut dedup_out = 0usize;

    // Normalize + filter by whitelist
    for mut ev in raw_events {
        ev.text = normalize_text(&ev.text);
        if !is_whitelisted(&ev.source, whitelist) {
            filtered_out += 1;
            continue;
        }
        filtered.push(ev);
    }

    // Deduplicate within window by exact text match of recent items.
    let mut seen_texts: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut keep = Vec::with_capacity(filtered.len());

    for ev in filtered.into_iter() {
        let is_recent = now.saturating_sub(ev.published_at) <= dedup_window_secs;
        if is_recent {
            if seen_texts.contains(&ev.text) {
                dedup_out += 1;
                continue;
            }
            seen_texts.insert(ev.text.clone());
        }
        keep.push(ev);
    }

    (keep, filtered_out, dedup_out)
}

/// E2E glue: fetch -> normalize/filter/dedup -> convert to your analyze/decide path.
/// Metrics:
///   - histogram ingest_fetch_duration_ms — vždy po fetchi (i při chybě)
///   - countery ingest_* — pouze pokud nastala „aktivita“ (aspoň jeden vstup/kept/filtered/dedup)
pub async fn ingest_and_decide<P: crate::ingest::types::SourceProvider>(
    p: &P,
    now: u64,
    whitelist: &[String],
    dedup_window_secs: u64,
) -> crate::decision::Decision {
    let start = std::time::Instant::now();
    let latest = match p.fetch_latest().await {
        Ok(v) => v,
        Err(e) => {
            // Měříme dobu fetch i při chybě, countery neemitujeme.
            histogram!("ingest_fetch_duration_ms").record(start.elapsed().as_millis() as f64);

            return crate::decision::Decision::hold(0.5).with_reason(format!(
                "ingest: provider {} error: {}",
                p.name(),
                e
            ));
        }
    };

    // Změříme samotný fetch (bez dalších kroků).
    histogram!("ingest_fetch_duration_ms").record(start.elapsed().as_millis() as f64);

    // normalize -> filter -> dedup
    let (events, filtered_cnt, dedup_cnt) =
        normalize_filter_dedup(now, latest, whitelist, dedup_window_secs);

    // Emitujeme countery až teď. „Aktivita“ = něco prošlo/odpadlo.
    let total_inputs = events.len() + filtered_cnt + dedup_cnt;
    if total_inputs > 0 {
        counter!("ingest_events_total").increment(total_inputs as u64);
        counter!("ingest_filtered_total").increment(filtered_cnt as u64);
        counter!("ingest_dedup_total").increment(dedup_cnt as u64);
    }

    // --- Adaptér do analyze/decide ---
    let joined: String = events
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if joined.is_empty() {
        return crate::decision::Decision::hold(0.5)
            .with_reason("ingest: no events after filter/dedup");
    }

    let sources: Vec<&str> = events.iter().map(|e| e.source.as_str()).collect();
    crate::decision::Decision::hold(0.5)
        .with_reason(format!(
            "ingest: {} events kept from {:?}",
            events.len(),
            sources
        ))
        .with_reason("TODO: wire to analyze/decide pipeline from Phase 1–5".to_string())
}
