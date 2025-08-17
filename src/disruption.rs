//! disruption.rs — Vyhodnocení „disruptivních“ výroků.
//!
//! Záměr: rychlá detekce šoků podle 3 složek:
//!   - w_source: váha důvěryhodnosti/importance zdroje (Trump, Fed, Yellen, ...).
//!   - w_strength: síla sentimentu (normalizace podle absolutní hodnoty skóre).
//!   - recency/age: čerstvost výroku (tvrdý práh < 30 min pro trigger).
//!
//! Pozn.: Zatím čistě „business logika“ bez I/O, bez side-effectů.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::source_weights::SourceWeightsConfig;

/// Konfigurační prahy — jednoduché a čitelné, ať je lze snadno ladit.
const TRIGGER_W_SOURCE_MIN: f32 = 0.80;
const TRIGGER_W_STRENGTH_MIN: f32 = 0.90;
const TRIGGER_MAX_AGE_SECS: u64 = 30 * 60; // 30 minut

/// Normalizační strop pro sílu výroku: |score| >= 3 → síla ~ 1.0.
/// (Později lze nahradit sofistikovanějším škálováním.)
const STRENGTH_CAP: i32 = 2;

/// Vstup pro vyhodnocení disruption — agregujeme potřebná pole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisruptionInput {
    pub source: String,
    pub text: String,
    pub score: i32,
    /// Unix timestamp v sekundách (kdy byl výrok publikován / zachycen).
    pub ts_unix: u64,
}

/// Výsledek vyhodnocení včetně složek; `triggered` říká, zda splněno.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisruptionResult {
    pub triggered: bool,
    pub w_source: f32,
    pub w_strength: f32,
    pub age_secs: u64,
}

impl DisruptionResult {
    pub fn not_triggered(w_source: f32, w_strength: f32, age_secs: u64) -> Self {
        Self {
            triggered: false,
            w_source,
            w_strength,
            age_secs,
        }
    }
    pub fn triggered(w_source: f32, w_strength: f32, age_secs: u64) -> Self {
        Self {
            triggered: true,
            w_source,
            w_strength,
            age_secs,
        }
    }
}

/// Hlavní funkce: vyhodnoť, zda jde o „disruptivní“ případ.
pub fn evaluate(input: &DisruptionInput) -> DisruptionResult {
    let now = now_unix();
    let age_secs = now.saturating_sub(input.ts_unix);

    // 1) Síla výroku podle absolutní hodnoty skóre.
    let w_strength = strength_weight(input.score);

    // 2) Váha zdroje (Top zdroje mají ≥ 0.8).
    let w_source = source_weight(&input.source);

    // 3) Tvrdý práh na čerstvost (musí být < 30 min).
    let is_fresh = age_secs <= TRIGGER_MAX_AGE_SECS;

    let passes =
        w_source >= TRIGGER_W_SOURCE_MIN && w_strength >= TRIGGER_W_STRENGTH_MIN && is_fresh;

    if passes {
        DisruptionResult::triggered(w_source, w_strength, age_secs)
    } else {
        DisruptionResult::not_triggered(w_source, w_strength, age_secs)
    }
}

/// Jednoduché škálování síly podle absolutního skóre.
pub fn strength_weight(score: i32) -> f32 {
    let s = (score.abs() as f32) / (STRENGTH_CAP as f32);
    clamp01(s)
}

/// Heuristika vah pro zdroje.
/// Později nahradíme tabulkou v JSON (konfigurovatelné bez rekompilace).
pub fn source_weight(source: &str) -> f32 {
    let s = source.trim().to_ascii_lowercase();
    match s.as_str() {
        "trump" => 0.95,
        "fed" => 0.90,
        "yellen" => 0.85,
        // default pro ostatní (analyst, media, apod.)
        _ => 0.60,
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn clamp01(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

pub fn evaluate_with_weights(input: &DisruptionInput, sw: &SourceWeightsConfig) -> DisruptionResult {
    let now = now_unix();
    let age_secs = now.saturating_sub(input.ts_unix);

    let w_strength = strength_weight(input.score);
    let w_source = clamp01(sw.weight_for(&input.source));

    let is_fresh = age_secs <= TRIGGER_MAX_AGE_SECS;

    let passes =
        w_source >= TRIGGER_W_SOURCE_MIN &&
        w_strength >= TRIGGER_W_STRENGTH_MIN &&
        is_fresh;

    if passes {
        DisruptionResult::triggered(w_source, w_strength, age_secs)
    } else {
        DisruptionResult::not_triggered(w_source, w_strength, age_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_trump_recent_triggers() {
        let now = super::now_unix();
        let inp = DisruptionInput {
            source: "Trump".into(),
            text: "The economy is strong.".into(),
            score: 3,     // síla ≈ 1.0
            ts_unix: now, // čerstvé
        };
        let res = evaluate(&inp);
        assert!(res.triggered);
        assert!(res.w_source >= 0.9);
        assert!(res.w_strength >= 0.9);
        assert!(res.age_secs <= TRIGGER_MAX_AGE_SECS);
    }

    #[test]
    fn weak_or_old_does_not_trigger() {
        let now = super::now_unix();
        // Slabé skóre
        let a = DisruptionInput {
            source: "Fed".into(),
            text: "We are monitoring.".into(),
            score: 1,
            ts_unix: now,
        };
        assert!(!evaluate(&a).triggered);

        // Staré (31 min)
        let b = DisruptionInput {
            source: "Trump".into(),
            text: "Strong statement.".into(),
            score: 3,
            ts_unix: now - (31 * 60),
        };
        assert!(!evaluate(&b).triggered);
    }
}
