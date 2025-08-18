//! # Decision Engine
//! Pure, testable logic that maps `(item, score, disruption)` → `Decision`.
//! No I/O, suitable for unit tests and future offline evaluation.
//!
//! Policy: dominant direction of *triggered* items yields BUY/SELL; conflicts
//! or lack of triggers yield HOLD. Confidence blends trigger count, average
//! component quality, and source independence.

use crate::decision::{Contributor, Decision, Reason, ReasonKind, Verdict};
use crate::disruption::DisruptionResult;
use crate::sentiment::BatchItem;

/// Same logic as the `/decide` handler but purely functional for testing.
pub fn make_decision(scored: &[(BatchItem, i32, DisruptionResult)]) -> Decision {
    // 1) Split triggered items into positive/negative
    let mut triggers_pos = Vec::new();
    let mut triggers_neg = Vec::new();

    for (it, score, res) in scored.iter() {
        if res.triggered {
            if *score > 0 {
                triggers_pos.push((it, *score, res));
            } else if *score < 0 {
                triggers_neg.push((it, *score, res));
            }
        }
    }

    // 2) Verdict with priorities (dominant direction → BUY/SELL; conflict → HOLD; else HOLD)
    let (verdict, main_triggers): (Verdict, Vec<(&BatchItem, i32, &DisruptionResult)>) =
        if !triggers_pos.is_empty() && triggers_neg.is_empty() {
            (
                Verdict::Buy,
                triggers_pos.iter().map(|(i, s, r)| (*i, *s, *r)).collect(),
            )
        } else if !triggers_neg.is_empty() && triggers_pos.is_empty() {
            (
                Verdict::Sell,
                triggers_neg.iter().map(|(i, s, r)| (*i, *s, *r)).collect(),
            )
        } else if !triggers_pos.is_empty() && !triggers_neg.is_empty() {
            (
                Verdict::Hold,
                if triggers_pos.len() >= triggers_neg.len() {
                    triggers_pos.iter().map(|(i, s, r)| (*i, *s, *r)).collect()
                } else {
                    triggers_neg.iter().map(|(i, s, r)| (*i, *s, *r)).collect()
                },
            )
        } else {
            (Verdict::Hold, Vec::new())
        };

    // 3) Confidence v3: base + trigger quality + independence bonus
    let confidence = if !main_triggers.is_empty() && verdict != Verdict::Hold {
        let k = main_triggers.len().min(2) as f32;

        let mut acc = 0.0f32;
        let mut uniq = std::collections::BTreeSet::new();
        for (it, _score, res) in main_triggers.iter() {
            acc += (res.w_source + res.w_strength) * 0.5;
            uniq.insert(it.source.as_str());
        }
        let avg = acc / (main_triggers.len() as f32);

        // Independence bonus (0–0.10): +0.05 per extra unique source (max +0.10)
        let independence_bonus = (uniq.len().saturating_sub(1) as f32).min(2.0) * 0.05;

        (0.60 + 0.15 * k + 0.10 * avg + independence_bonus).min(0.95)
    } else {
        0.55
    };

    // 4) Reasons
    let mut reasons = Vec::new();
    if !main_triggers.is_empty() {
        // 4a) Explicit confirmation that thresholds were met (ASCII for stable console output)
        for (it, _score, res) in main_triggers.iter().take(3) {
            let msg = format!(
                "Trigger met: source>=0.80, strength>=0.90, age<=1800s (actual: w_source {:.2}, w_strength {:.2}, age {}s) - {}",
                res.w_source, res.w_strength, res.age_secs, it.source
            );
            reasons.push(
                Reason::new(msg)
                    .kind(ReasonKind::Threshold)
                    .weighted(((res.w_source + res.w_strength) / 2.0).min(1.0)),
            );
        }

        // 4b) Human-readable citation (keeps the current style)
        for (it, score, res) in main_triggers.iter().take(3) {
            let msg = format!(
                "{}: \"{}\" (score {:+}, w_source {:.2}, w_strength {:.2}, age {}s)",
                it.source, it.text, score, res.w_source, res.w_strength, res.age_secs
            );
            reasons.push(
                Reason::new(msg)
                    .kind(ReasonKind::Threshold)
                    .weighted(((res.w_source + res.w_strength) / 2.0).min(1.0)),
            );
        }
    } else {
        reasons.push(
            Reason::new("No disruptive statements within the last 30 minutes.")
                .kind(ReasonKind::Threshold)
                .weighted(0.4),
        );
    }

    // 5) Top contributors (Top 3: triggered items are boosted; then by |score|)
    let mut all = scored
        .iter()
        .map(|(it, score, res)| (it, *score, res))
        .collect::<Vec<_>>();
    all.sort_by_key(|(_, score, res)| {
        let boost = if res.triggered { 1000 } else { 0 };
        boost + score.abs()
    });
    all.reverse();

    let mut contributors = Vec::new();
    for (it, score, res) in all.into_iter().take(3) {
        contributors.push(
            Contributor::new(&it.source, &it.text, score, iso_now()).weights(
                res.w_source,
                res.w_strength,
                recency_weight(res.age_secs),
            ),
        );
    }

    Decision {
        decision: verdict,
        confidence,
        reasons,
        top_contributors: contributors,
    }
}

/// Soft, linear decay from 0..1800s (inclusive).
fn recency_weight(age_secs: u64) -> f32 {
    if age_secs == 0 {
        1.0
    } else {
        ((1800.0 - (age_secs as f32)).max(0.0)) / 1800.0
    }
}

/// Minimal ISO-like timestamp as `String` (keep dependencies at zero).
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disruption::DisruptionResult;

    fn mk_item(src: &str, txt: &str) -> BatchItem {
        BatchItem {
            source: src.to_string(),
            text: txt.to_string(),
        }
    }
    fn trig(w_source: f32, w_strength: f32, age: u64) -> DisruptionResult {
        DisruptionResult {
            triggered: true,
            w_source,
            w_strength,
            age_secs: age,
        }
    }
    fn notrig(w_source: f32, w_strength: f32, age: u64) -> DisruptionResult {
        DisruptionResult {
            triggered: false,
            w_source,
            w_strength,
            age_secs: age,
        }
    }

    #[test]
    fn buy_on_strong_positive_trigger() {
        let items = vec![
            (mk_item("Trump", "Economy strong"), 2, trig(0.95, 1.0, 10)),
            (mk_item("Analyst", "blah"), 0, notrig(0.6, 0.0, 10)),
        ];
        let d = make_decision(&items);
        assert_eq!(d.decision, Verdict::Buy);
        assert!(d.confidence >= 0.75 && d.confidence <= 0.95);
        assert!(!d.reasons.is_empty());
    }

    #[test]
    fn sell_on_strong_negative_trigger() {
        let items = vec![(mk_item("Fed", "Plunge incoming"), -2, trig(0.90, 1.0, 5))];
        let d = make_decision(&items);
        assert_eq!(d.decision, Verdict::Sell);
    }

    #[test]
    fn hold_on_conflict() {
        let items = vec![
            (mk_item("Trump", "Up!"), 2, trig(0.95, 1.0, 20)),
            (mk_item("Fed", "Down"), -2, trig(0.90, 1.0, 15)),
        ];
        let d = make_decision(&items);
        assert_eq!(d.decision, Verdict::Hold);
        // Confidence should be low for conflicts.
        assert!(d.confidence <= 0.60);
    }

    #[test]
    fn hold_without_triggers() {
        let items = vec![(mk_item("Analyst", "meh"), 0, notrig(0.6, 0.0, 300))];
        let d = make_decision(&items);
        assert_eq!(d.decision, Verdict::Hold);
    }
}
