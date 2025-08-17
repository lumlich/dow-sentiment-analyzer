//! disruption.rs — Vyhodnocení „disruptivních“ výroků.
//!
//! Záměr: rychlá detekce šoků podle 3 složek:
//!   - w_source: váha důvěryhodnosti/importance zdroje (Trump, Fed, Yellen, ...).
//!   - w_strength: síla sentimentu (normalizace podle absolutní hodnoty skóre).
//!   - recency/age: čerstvost výroku (měkký pokles vlivu mezi 15–30 min).
//!
//! Pozn.: Čistě „business logika“ bez I/O, bez side-effectů.

use crate::source_weights::SourceWeightsConfig;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Konfigurační prahy — jednoduché a čitelné, ať je lze snadno ladit.
const TRIGGER_W_SOURCE_MIN: f32 = 0.80;
const TRIGGER_W_STRENGTH_MIN: f32 = 0.90;
pub const TRIGGER_MAX_AGE_SECS: u64 = 30 * 60; // 30 minut
const RECENCY_SOFT_START_SECS: u64 = 15 * 60; // 15 min (odtud začneme pozvolna tlumit)

/// Normalizační strop pro sílu výroku: |score| >= 2 → síla ~ 1.0.
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

/// Měkká recency váha: do 15 min 1.0, mezi 15–30 min lineárně klesá na 0.0, poté 0.0.
fn recency_weight(age_secs: u64) -> f32 {
    if age_secs <= RECENCY_SOFT_START_SECS {
        1.0
    } else if age_secs <= TRIGGER_MAX_AGE_SECS {
        let span = (TRIGGER_MAX_AGE_SECS - RECENCY_SOFT_START_SECS) as f32; // 900 s
        let over = (age_secs - RECENCY_SOFT_START_SECS) as f32;
        (1.0 - over / span).max(0.0)
    } else {
        0.0
    }
}

/// Hlavní funkce: vyhodnoť, zda jde o „disruptivní“ případ (bez externích vah).
pub fn evaluate(input: &DisruptionInput) -> DisruptionResult {
    let now = now_unix();
    let age_secs = now.saturating_sub(input.ts_unix);

    // 1) Síla výroku podle absolutní hodnoty skóre.
    let w_strength = strength_weight(input.score);

    // 2) Váha zdroje (heuristika; verze s externím konfigem viz evaluate_with_weights).
    let w_source = source_weight(&input.source);

    // 3) Měkké stáří výroku (plynule tlumíme vliv po 15. minutě).
    let w_recency = recency_weight(age_secs);

    // Trigger jen pokud recency > 0 (tj. ≤ 30 min), a splněny prahy zdroj/síla.
    let passes = w_source >= TRIGGER_W_SOURCE_MIN
        && w_strength >= TRIGGER_W_STRENGTH_MIN
        && w_recency > 0.0;

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

/// Heuristika vah pro zdroje (fallback; v produkci používáme evaluate_with_weights).
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

pub fn now_unix() -> u64 {
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

/// Varianta s externími vahami (konfigurovatelné bez rekompilace).
pub fn evaluate_with_weights(input: &DisruptionInput, sw: &SourceWeightsConfig) -> DisruptionResult {
    let now = now_unix();
    let age_secs = now.saturating_sub(input.ts_unix);

    let w_strength = strength_weight(input.score);
    let w_source = clamp01(sw.weight_for(&input.source));
    let w_recency = recency_weight(age_secs);

    let passes = w_source >= TRIGGER_W_SOURCE_MIN
        && w_strength >= TRIGGER_W_STRENGTH_MIN
        && w_recency > 0.0;

    if passes {
        DisruptionResult::triggered(w_source, w_strength, age_secs)
    } else {
        DisruptionResult::not_triggered(w_source, w_strength, age_secs)
    }
}

//
// ------------------------------ TESTY ----------------------------------
//

#[cfg(test)]
mod tests {
    use super::{evaluate, now_unix, DisruptionInput, TRIGGER_MAX_AGE_SECS};

    #[test]
    fn strong_trump_recent_triggers() {
        let now = now_unix();
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
        let now = now_unix();
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

#[cfg(test)]
mod weight_integration_tests {
    use super::{evaluate_with_weights, now_unix, DisruptionInput};
    use crate::source_weights::SourceWeightsConfig;
    use std::collections::HashMap;

    fn cfg_with(source: &str, w: f32) -> SourceWeightsConfig {
        let mut weights = HashMap::new();
        // ulož kanonicky – lowercase
        weights.insert(source.to_ascii_lowercase(), w);
        SourceWeightsConfig {
            default_weight: 0.60,
            weights,
            aliases: HashMap::new(),
        }
    }

    #[test]
    fn triggers_when_weight_and_strength_meet_thresholds() {
        // w_source 0.90, score +2 => w_strength ~1.0, age 0 => mělo by TRIGGERNOUT
        let cfg = cfg_with("BigSource", 0.90);
        let input = DisruptionInput {
            source: "BigSource".into(),
            text: "Strong surge".into(),
            score: 2, // díky STRENGTH_CAP=2 => w_strength=1.0
            ts_unix: now_unix(),
        };
        let res = evaluate_with_weights(&input, &cfg);
        assert!(res.triggered, "mělo to triggernout");
        assert!(res.w_source >= 0.90);
        assert!(res.w_strength >= 0.90);
    }

    #[test]
    fn does_not_trigger_if_source_weight_too_low() {
        // w_source 0.70 (pod prahem), jinak silné => NEMÁ triggernout
        let cfg = cfg_with("LowSource", 0.70);
        let input = DisruptionInput {
            source: "LowSource".into(),
            text: "Strong surge".into(),
            score: 2,
            ts_unix: now_unix(),
        };
        let res = evaluate_with_weights(&input, &cfg);
        assert!(!res.triggered, "nemělo to triggernout kvůli nízké w_source");
        assert!(res.w_source < 0.80);
        assert!(res.w_strength >= 0.90); // síla je OK, ale zdroj zablokuje
    }

    #[test]
    fn does_not_trigger_if_too_old() {
        let cfg = cfg_with("Fed", 0.95);
        let old_ts = now_unix().saturating_sub(31 * 60); // 31 min zpět
        let input = DisruptionInput {
            source: "Fed".into(),
            text: "Markets will crash".into(),
            score: -3,
            ts_unix: old_ts,
        };
        let res = evaluate_with_weights(&input, &cfg);
        assert!(!res.triggered, "nemělo to triggernout kvůli stáří výroku");
        assert!(res.w_source >= 0.90);
        assert!(res.w_strength >= 0.90);
        assert!(res.age_secs > 1800);
    }
}

#[cfg(test)]
mod recency_tests {
    use super::{evaluate_with_weights, now_unix, DisruptionInput};
    use crate::source_weights::SourceWeightsConfig;

    #[test]
    fn recency_soft_taper_between_15_and_30_min() {
        let now = now_unix();
        let inp_20m = DisruptionInput {
            source: "Fed".into(),
            text: "Strong statement".into(),
            score: 3,
            ts_unix: now - (20 * 60),
        };
        let res = evaluate_with_weights(&inp_20m, &SourceWeightsConfig::default_seed());
        // Má to projít (≤ 30 min), jen s nižším recency weightem
        assert!(res.triggered);
    }

    #[test]
    fn recency_above_30_min_should_not_trigger() {
        let now = now_unix();
        let inp_31m = DisruptionInput {
            source: "Fed".into(),
            text: "Strong statement".into(),
            score: 3,
            ts_unix: now - (31 * 60),
        };
        let res = evaluate_with_weights(&inp_31m, &SourceWeightsConfig::default_seed());
        assert!(!res.triggered);
    }
}

#[cfg(test)]
mod reload_like_test {
    use crate::source_weights::SourceWeightsConfig;
    use std::sync::{Arc, RwLock};

    #[test]
    fn rwlock_update_config_works() {
        let initial = SourceWeightsConfig::default_seed();
        let lock = Arc::new(RwLock::new(initial));

        // čtení
        {
            let g = lock.read().unwrap();
            assert!((g.weight_for("Trump") - 0.98).abs() < 1e-6);
        }

        // zápis nového cfg (změňme třeba Trump->0.80)
        let mut new = SourceWeightsConfig::default_seed();
        new.weights.insert("trump".to_string(), 0.80);

        {
            let mut w = lock.write().unwrap();
            *w = new;
        }

        // ověř novou váhu
        {
            let g = lock.read().unwrap();
            assert!((g.weight_for("Trump") - 0.80).abs() < 1e-6);
        }
    }
}