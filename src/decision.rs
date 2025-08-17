//! decision.rs — Struktury pro verdict, explainability a top contributors.
//!/! Cíl Fáze 1: mít standardizovaný výstup pro BUY/HOLD/SELL + confidence + důvody,
//! aby bylo snadné později přidat rolling okno, disruption detector a confidence výpočet.
//!
//! Pozn.: Aplikace je zaměřená na "disruptivní" výroky (šoky). Rolling metriky jsou
//! informativní, ne-triggery alertů (notifikace spouští až disruption logika).

use serde::{Deserialize, Serialize};

/// Verdikt rozhodnutí pro akci na trhu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Buy,
    Hold,
    Sell,
}

/// Důvod, který se zobrazí uživateli (explainability).
/// Jednoduché a čitelné; v dalších krocích můžeme přidat jemnější členění.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    /// Lidsky čitelný popis (např. "Trump said economy is strong (+2)").
    pub message: String,
    /// Volitelná váha důvodu v intervalu <0.0, 1.0> (pokud dává smysl ji uvádět).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    /// Volitelná kategorie důvodu (source_strength, recency, consensus, volume, rolling_trend, threshold, other).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ReasonKind>,
}

/// Hrubé typy důvodů — pomůže s konzistencí v UI a testech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    SourceStrength,
    Recency,
    Consensus,
    Volume,
    RollingTrend,
    Threshold,
    Other,
}

/// Nejvýznamnější přispěvatelé k aktuálnímu verdiktu.
/// Umožňuje zobrazit "evidence": kdo co řekl, jaký to mělo sentiment a kdy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contributor {
    /// Např. "Trump", "Fed", "Yellen", "Analyst", ...
    pub source: String,
    /// Původní krátký text výroku / zprávy.
    pub text: String,
    /// Výsledné lexikonové skóre (sumární integer).
    pub score: i32,
    /// Čas výroku — ISO 8601 doporučen (např. "2025-08-16T10:00:00Z").
    /// Necháváme jako String, abychom nemuseli přidávat chrono; naplní se při zpracování.
    #[serde(rename = "ts")]
    pub ts_iso: String,

    /// Volitelné dílčí váhy použité při disruption detekci.
    /// Přidáváme teď kvůli budoucí explainabilitě; nemusí být vždy vyplněné.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_source: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w_recency: Option<f32>,
}

/// Kompletní rozhodnutí včetně explainability.
/// Toto je shape, který bude API vracet po rozšíření endpointů.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub decision: Verdict,
    /// Jistota rozhodnutí v intervalu <0.0, 1.0>.
    pub confidence: f32,
    /// Stručný, ale užitečný seznam důvodů.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<Reason>,
    /// Top N přispěvatelů (typicky 1–3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_contributors: Vec<Contributor>,
}

impl Decision {
    /// Vytvoří prázdné (skeletální) rozhodnutí s daným verdiktem a confidence.
    pub fn new(verdict: Verdict, confidence: f32) -> Self {
        Self {
            decision: verdict,
            confidence: clamp01(confidence),
            reasons: Vec::new(),
            top_contributors: Vec::new(),
        }
    }

    /// Pohodlné konstruktory pro nejčastější varianty.
    pub fn buy(confidence: f32) -> Self {
        Self::new(Verdict::Buy, confidence)
    }
    pub fn hold(confidence: f32) -> Self {
        Self::new(Verdict::Hold, confidence)
    }
    pub fn sell(confidence: f32) -> Self {
        Self::new(Verdict::Sell, confidence)
    }

    /// Přidá jeden důvod (builder styl).
    pub fn with_reason(mut self, message: impl Into<String>) -> Self {
        self.reasons.push(Reason {
            message: message.into(),
            weight: None,
            kind: None,
        });
        self
    }

    /// Přidá contributor (builder styl).
    pub fn with_contributor(mut self, c: Contributor) -> Self {
        self.top_contributors.push(c);
        self
    }
}

impl Reason {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            weight: None,
            kind: None,
        }
    }

    pub fn weighted(mut self, w: f32) -> Self {
        self.weight = Some(clamp01(w));
        self
    }

    pub fn kind(mut self, kind: ReasonKind) -> Self {
        self.kind = Some(kind);
        self
    }
}

impl Contributor {
    pub fn new(
        source: impl Into<String>,
        text: impl Into<String>,
        score: i32,
        ts_iso: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            text: text.into(),
            score,
            ts_iso: ts_iso.into(),
            w_source: None,
            w_strength: None,
            w_recency: None,
        }
    }

    pub fn weights(mut self, w_source: f32, w_strength: f32, w_recency: f32) -> Self {
        self.w_source = Some(clamp01(w_source));
        self.w_strength = Some(clamp01(w_strength));
        self.w_recency = Some(clamp01(w_recency));
        self
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
fn serialize_decision_shape_matches_roadmap() {
    let d = Decision::buy(0.78)
        .with_reason("Trump said economy is strong (+2)")
        .with_reason("Fed cautious (-1)")
        .with_contributor(
            Contributor::new(
                "Trump",
                "The economy is strong.",
                2,
                "2025-08-16T10:00:00Z",
            )
            .weights(0.95, 0.92, 1.0),
        );

    let v: serde_json::Value = serde_json::to_value(&d).unwrap();

    // Klíčové klíče podle roadmapy
    assert_eq!(v["decision"], serde_json::json!("BUY"));

    // ← místo assert_eq! na float použij aproximaci
    let conf = v["confidence"].as_f64().unwrap();
    assert!((conf - 0.78).abs() < 1e-6, "confidence ~= 0.78, got {}", conf);

    assert!(v["reasons"].is_array());
    assert!(v["top_contributors"].is_array());

    // Minimálně 1 contributor s očekávanými poli
    let c = &v["top_contributors"][0];
    assert_eq!(c["source"], serde_json::json!("Trump"));
    assert_eq!(c["text"], serde_json::json!("The economy is strong."));
    assert_eq!(c["score"], serde_json::json!(2));
    assert_eq!(c["ts"], serde_json::json!("2025-08-16T10:00:00Z"));
}
}
