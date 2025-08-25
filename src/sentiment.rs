//! # Sentiment Analysis Domain
//! Lexicon-based scoring with simple negation handling.
//!
//! - Tokenizes text into lowercase alphanumeric words.
//! - Each token is looked up in a static sentiment lexicon (`HashMap<String, i32>`).
//! - Negation: if a negator appears in the last 1–3 tokens, the score of the
//!   current token is inverted.
//!
//! Pure functions, no I/O; suitable for testing and reuse.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Static lexicon loaded at startup from `sentiment_lexicon.json`.
static LEXICON: Lazy<HashMap<String, i32>> = Lazy::new(|| {
    let raw = include_str!("../sentiment_lexicon.json");
    serde_json::from_str::<HashMap<String, i32>>(raw).expect("valid sentiment lexicon")
});

/// Stateless sentiment analyzer (lexicon-based).
#[derive(Debug, Clone, Default)]
pub struct SentimentAnalyzer;

impl SentimentAnalyzer {
    /// Construct a new analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Internal helper: return the lexicon score for a word (`0` if not in lexicon).
    #[inline]
    fn word_score(&self, w: &str) -> i32 {
        *LEXICON.get(w).unwrap_or(&0)
    }

    /// Score a text and return `(score, token_count)`.
    ///
    /// Negation handling: if any negator is found in the last 1–3 tokens before
    /// a word, its score is inverted.
    ///
    /// # Example
    /// ```
    /// use dow_sentiment_analyzer::sentiment::SentimentAnalyzer;
    ///
    /// let sa = SentimentAnalyzer::new();
    /// let (s, n) = sa.score_text("good job");
    ///
    /// assert!(s > 0);
    /// assert_eq!(n, 2);
    /// ```
    pub fn score_text(&self, text: &str) -> (i32, usize) {
        // Tokenize into a Vec so we can look back for negation.
        let tokens: Vec<String> = tokenize(text).collect();
        let mut score: i32 = 0;

        for i in 0..tokens.len() {
            let w = tokens[i].as_str();

            // Check if a negator is within the last 1–3 tokens.
            let negated = (1..=3).any(|k| i >= k && is_negator(tokens[i - k].as_str()));

            let base = self.word_score(w);
            if base != 0 {
                let adj = if negated { -base } else { base };
                score += adj;
            }
        }

        (score, tokens.len())
    }
}

/// Tokenizer: split on non-alphanumeric chars, lowercase all tokens.
fn tokenize(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
}

/// Simple set of negators.
/// Covers single tokens only ("no longer" is covered by "no").
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
    )
}

/// Batch input item: source + text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub source: String,
    pub text: String,
}
