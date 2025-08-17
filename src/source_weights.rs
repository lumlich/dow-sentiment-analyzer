//! source_weights.rs — načítání a lookup vah zdrojů z JSON configu.

use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct SourceWeightsConfig {
    #[serde(default = "default_default_weight")]
    pub default_weight: f32,
    #[serde(default)]
    pub weights: HashMap<String, f32>,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

fn default_default_weight() -> f32 {
    0.60
}

impl SourceWeightsConfig {
    /// Načte JSON config ze souboru. Při chybě vrátí „rozumné“ defaulty.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| Self::default_seed()),
            Err(_) => Self::default_seed(),
        }
    }

    /// Získá váhu pro daný zdroj (case-insensitive, s aliasy a jednoduchým substring fallbackem).
    pub fn weight_for(&self, source: &str) -> f32 {
        let s = normalize(source);

        // 1) Přesná shoda v aliases → canonical → weights
        if let Some(canon) = self.aliases.get(&s) {
            let c = normalize(canon);
            if let Some(&w) = self.weights.get(&c) {
                return clamp01(w);
            }
        }

        // 2) Přesná shoda ve weights
        if let Some(&w) = self.weights.get(&s) {
            return clamp01(w);
        }

        // 3) Jednoduchý substring fallback (např. "The Wall Street Journal" → "wall street journal")
        for (k, &w) in &self.weights {
            if s.contains(k) {
                return clamp01(w);
            }
        }

        // 4) Default
        clamp01(self.default_weight)
    }

    /// Vestavěný seed (když není config nebo je rozbitý).
    fn default_seed() -> Self {
        let mut weights = HashMap::new();
        let mut aliases = HashMap::new();

        for (k, v) in [
            ("trump", 0.98),
            ("biden", 0.95),
            ("powell", 0.97),
            ("fed", 0.95),
            ("fomc", 0.95),
            ("yellen", 0.93),
            ("musk", 0.97),
            ("xi", 0.95),
            ("wsj", 0.90),
            ("wall street journal", 0.90),
            ("reuters", 0.85),
            ("bloomberg", 0.85),
            ("financial times", 0.90),
            ("new york times", 0.88),
            ("apple", 0.90),
            ("microsoft", 0.90),
            ("alphabet", 0.88),
            ("google", 0.88),
            ("meta", 0.88),
            ("facebook", 0.88),
            ("jamie dimon", 0.90),
            ("jpmorgan", 0.88),
            ("blackrock", 0.86),
            ("goldman sachs", 0.86),
            ("vanguard", 0.84),
            ("ecb", 0.86),
            ("boe", 0.84),
        ] {
            weights.insert(k.to_string(), v);
        }

        for (a, c) in [
            ("@realdonaldtrump", "trump"),
            ("president", "biden"),
            ("potus", "biden"),
            ("jerome powell", "powell"),
            ("federal reserve", "fed"),
            ("janet yellen", "yellen"),
            ("u.s. treasury", "yellen"),
            ("treasury", "yellen"),
            ("elon", "musk"),
            ("@elonmusk", "musk"),
            ("xi jinping", "xi"),
            ("xijinping", "xi"),
            ("the wall street journal", "wall street journal"),
            ("wsj.com", "wsj"),
            ("ft", "financial times"),
            ("nytimes", "new york times"),
            ("nyt", "new york times"),
            ("google inc", "google"),
            ("alphabet inc", "alphabet"),
            ("meta platforms", "meta"),
            ("facebook inc", "facebook"),
            ("jp morgan", "jpmorgan"),
            ("j.p. morgan", "jpmorgan"),
            ("gs", "goldman sachs"),
            ("european central bank", "ecb"),
            ("bank of england", "boe"),
        ] {
            aliases.insert(a.to_string(), c.to_string());
        }

        Self {
            default_weight: 0.60,
            weights,
            aliases,
        }
    }
}

fn normalize(s: &str) -> String {
    s.trim()
        .to_ascii_lowercase()
        .replace(['\n', '\r', '\t'], " ")
        .replace(['—', '–'], "-") // vyhladíme typografii
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

    fn cfg() -> SourceWeightsConfig {
        SourceWeightsConfig::default_seed()
    }

    #[test]
    fn exact_match() {
        let c = cfg();
        assert!((c.weight_for("Trump") - 0.98).abs() < 1e-6);
    }
    #[test]
    fn alias_match() {
        let c = cfg();
        assert!((c.weight_for("@elonmusk") - 0.97).abs() < 1e-6);
        assert!((c.weight_for("Jerome Powell") - 0.97).abs() < 1e-6);
    }
    #[test]
    fn substring_match() {
        let c = cfg();
        assert!((c.weight_for("The Wall Street Journal") - 0.90).abs() < 1e-6);
    }
    #[test]
    fn default_weight_used() {
        let c = cfg();
        assert!((c.weight_for("TotallyUnknown") - c.default_weight).abs() < 1e-6);
    }
}
