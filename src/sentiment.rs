use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static LEXICON: Lazy<HashMap<String, i32>> = Lazy::new(|| {
    let raw = include_str!("../sentiment_lexicon.json");
    serde_json::from_str::<HashMap<String, i32>>(raw).expect("valid sentiment lexicon")
});

#[derive(Debug, Clone)]
pub struct SentimentAnalyzer;

impl SentimentAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Lokální helper: vrátí lexikonové skóre pro slovo (0 pokud není ve slovníku).
    #[inline]
    fn word_score(&self, w: &str) -> i32 {
        *LEXICON.get(w).unwrap_or(&0)
    }

    /// Vrací (score, počet tokenů).
    /// Negace: pokud se v posledních 1..=3 tokenech objeví negátor,
    /// invertujeme znamení lexikonového skóre daného slova.
    pub fn score_text(&self, text: &str) -> (i32, usize) {
        // Použij modulovou `tokenize` a nasbírej do vektoru,
        // protože potřebujeme indexovat zpětně kvůli negaci.
        let tokens: Vec<String> = tokenize(text).collect();
        let mut score: i32 = 0;

        for i in 0..tokens.len() {
            let w = tokens[i].as_str();

            // je v posledních 1..=3 tokenech negátor?
            let negated = (1..=3).any(|k| i >= k && is_negator(tokens[i - k].as_str()));

            let base = self.word_score(w);
            if base != 0 {
                // invertuj znamení, pokud je negace poblíž
                let adj = if negated { -base } else { base };
                score += adj;
            }
        }

        (score, tokens.len())
    }
}

/// Modulová tokenizace: alfanumerické tokeny, lower-case.
fn tokenize(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
}

/// Jednoduchá množina negátorů (stačí jednokolové – „no longer“ pokryje už samotné „no“).
fn is_negator(tok: &str) -> bool {
    matches!(
        tok,
        "not"
            | "no"
            | "never"
            | "isn't"
            | "wasn't"
            | "aren't"
            | "won't"
            | "can't"
            | "cannot"
            | "without"
            // „no longer“ řešíme už „no“, protože tokenizace to rozdělí
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub source: String,
    pub text: String,
}