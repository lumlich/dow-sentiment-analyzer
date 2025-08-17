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

    /// Vrací (score, počet tokenů).
    pub fn score_text(&self, text: &str) -> (i32, usize) {
        let mut score = 0i32;
        let mut tokens = 0usize;

        for token in tokenize(text) {
            tokens += 1;
            if let Some(s) = LEXICON.get(&token) {
                score += *s as i32;
            }
        }
        (score, tokens)
    }
}

fn tokenize(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub source: String,
    pub text: String,
}
