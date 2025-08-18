# dow-sentiment-analyzer
A sentiment analysis and decision engine for Dow Jones futures, built with Rust, Axum, and Tokio.

It processes short texts (e.g., statements by Trump, the Fed, Yellen, Reuters, etc.), scores sentiment with a small lexicon + negation handling, applies **per-source weights**, and produces a transparent **BUY / HOLD / SELL** decision with confidence and human-readable reasons.

---

## Features
- Lexicon-based sentiment with simple negation (e.g., “not good”).
- Configurable **source weights** via `source_weights.json` (+ aliases).
- **Disruption** detection (source strength × sentiment × recency).
- Confidence calibration with recent **volume context** (last 10 minutes).
- Rolling metrics (48h average & count) and in-memory decision **history**.
- Clean JSON API + debug endpoints.

---

## Quickstart (local dev with Shuttle)
```bash
# clone and enter
git clone https://github.com/lumlich/dow-sentiment-analyzer.git
cd dow-sentiment-analyzer

# run tests & lints (optional but recommended)
cargo fmt
cargo clippy -- -D warnings
cargo test

# run locally (Shuttle dev runtime)
cargo shuttle run
