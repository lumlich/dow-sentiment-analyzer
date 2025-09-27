dow-sentiment-analyzer — v0.4.0 (Phase 6: Ingest & Metrics)
====================================================================

[CI badge]   https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml/badge.svg
[Audit badge] https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml/badge.svg

A sentiment analysis and decision engine for Dow Jones futures, built with Rust, Axum, and Tokio.

It processes short texts (e.g., statements by Trump, the Fed, Yellen, Reuters, etc.), scores sentiment with a small
lexicon + negation handling, applies per-source weights, and produces a transparent BUY / HOLD / SELL decision with
confidence and human‑readable reasons.

--------------------------------------------------------------------
Features
--------------------------------------------------------------------
- Lexicon-based sentiment with simple negation (e.g., “not good”).
- Configurable source weights via `source_weights.json` (+ aliases).
- Disruption detection (source strength × sentiment × recency).
- Confidence calibration with recent volume context (last 10 minutes).
- Rolling metrics (48h average & count) and in-memory decision history.
- Clean JSON API + debug endpoints.
- Slack + Discord webhook notifications (configurable).
- **Optional AI integration with caching and daily call limits.**
- **Phase 6:** Ingest (fixtures-first) + Prometheus metrics for ingest.

--------------------------------------------------------------------
Quickstart (local dev with Shuttle)
--------------------------------------------------------------------
# clone and enter
git clone https://github.com/lumlich/dow-sentiment-analyzer.git
cd dow-sentiment-analyzer

# run tests & lints
cargo fmt
cargo clippy -- -D warnings
cargo test

# run locally (Shuttle dev runtime) – use this (do not use `cargo run`)
cargo shuttle run
Note: The service runs under Shuttle's local runtime. Use `cargo shuttle run` instead of `cargo run`.

--------------------------------------------------------------------
Usage (API examples)
--------------------------------------------------------------------
GET /health  (root)
  curl -s http://localhost:8000/health
  -> OK

GET /api/ping
  curl -s http://localhost:8000/api/ping
  -> pong

POST /api/analyze
  curl -s -X POST http://localhost:8000/api/analyze \
    -H "Content-Type: application/json" \
    -d '{"text":"Fed signals a cautious path to rate cuts this year.","source":"Fed"}'

POST /api/batch
  curl -s -X POST http://localhost:8000/api/batch \
    -H "Content-Type: application/json" \
    -d '[{"id":"a1","text":"Trump says Dow will soar.","source":"Trump"},
         {"id":"b2","text":"Reuters: unexpected slowdown in manufacturing.","source":"Reuters"}]'

POST /api/decide
  curl -s -X POST http://localhost:8000/api/decide \
    -H "Content-Type: application/json" \
    -d '[{"source":"Reuters","text":"ISM manufacturing dips below 50; the Dow slips."}]'

Example response:
{
  "decision": "SELL",
  "confidence": 0.68,
  "reasons": [
    "macro+hard combo matched",
    "Relevance gate passed with score 0.47"
  ]
}

If irrelevant (e.g., DJI drones):
{
  "decision": "NEUTRAL",
  "reasons": ["neutralized: below relevance threshold"]
}

Windows / PowerShell tip: `curl` is an alias for `Invoke-WebRequest`. Use either `curl.exe` (actual curl) or PowerShell cmdlets:
  $body = '[{"source":"Fed","text":"Powell hints at uncertainty"}]'
  Invoke-WebRequest -Method POST -Uri "http://127.0.0.1:8000/api/decide" `
    -ContentType "application/json" -Body $body -UseBasicParsing

--------------------------------------------------------------------
Development — Common tasks
--------------------------------------------------------------------
Cargo aliases (see `.cargo/config.toml`):

- `cargo t`  → run all fast unit tests
- `cargo tu` → run unit tests only in this crate
- `cargo ts` → run synthetic suite (marked `#[ignore]`)
- `cargo cf` → check formatting
- `cargo cl` → run Clippy with `-D warnings`

--------------------------------------------------------------------
Phase 6 — Ingest (HTTP + Fixtures)
--------------------------------------------------------------------
Features / cargo features:

  Feature            What it does                                             Default
  -----------------------------------------------------------------------------------
  ingest-fixtures    Build providers from local XML fixtures (no network)     ON
  ingest-http        Enable HTTP-backed providers (from_url, fetch_latest)    OFF
  strict-metrics     Compile strict ingest metrics test                       OFF
  strict-e2e         Compile a strict E2E smoke test for `/decide`            OFF

Defaults: `default = ["ingest-fixtures"]`. CI does not make network calls.

HTTP ingest (opt‑in) — providers support both fixtures and HTTP:

#[cfg(feature = "ingest-http")]
{
    use dow_sentiment_analyzer::ingest::providers::{
        fed_rss::FedRssProvider,
        reuters_rss::ReutersRssProvider,
    };

    let fed     = FedRssProvider::from_url("https://www.federalreserve.gov/feeds/press_all.xml");
    let reuters = ReutersRssProvider::from_url("https://feeds.reuters.com/reuters/businessNews");

    // Example (do not call in CI):
    // let fed_items = fed.fetch_latest().await?;
    // let reu_items = reuters.fetch_latest().await?;
}

Telemetry on HTTP errors:
- Logs `warn!` with provider name
- Increments `ingest_provider_errors_total`

End-to-end (fixtures → analyze → verdict)
  cargo test --test ingest_e2e_decision

Strict E2E for `/decide` (opt-in):
  cargo test --features "strict-e2e" --test ingest_e2e -- --nocapture

Build & lint with HTTP enabled (compilation-only):
  cargo check  --features ingest-http
  cargo clippy --features ingest-http -- -D warnings
  cargo test   --no-run --features ingest-http

--------------------------------------------------------------------
Strict Metrics Test (feature-gated)
--------------------------------------------------------------------
A stricter ingest metrics test is available behind an optional Cargo feature.
By default, it does not compile nor run so your local and CI suites stay green.

Run default suite:
  cargo test

Run with strict metrics enabled:
  cargo test --features strict-metrics

--------------------------------------------------------------------
Relevance Gate (overview)
--------------------------------------------------------------------
- Anchors — strong patterns like `djia|dow jones|the dow` or `powell` near `fed|fomc|rates?`.
- Blockers — exclude false positives (`dji drones`, `dow inc`).
- Proximity rules — `near { pattern, window }` for contextual matches.
- Combos — pass conditions, e.g. need both `macro` and `hard`.
- Weights — category weights (e.g., `hard=3, macro=2, semi=2, soft=1`) combine into the score.

Env vars:
  RELEVANCE_CONFIG_PATH  (default: config/relevance.toml)
  RELEVANCE_THRESHOLD    (default: 0.30)
  RELEVANCE_HOT_RELOAD=1 (dev only)
  RELEVANCE_DEV_LOG=1    (dev only)

--------------------------------------------------------------------
Notifications (Phase 5 recap)
--------------------------------------------------------------------
- Decision changes (BUY ↔ SELL, HOLD transitions) are notified.
- Antiflutter cooldown prevents spam.
- Channels: Slack (webhook), Discord (webhook), Email (optional).
- Env: DECIDE_URL, NOTIFY_INTERVAL_SECS, NOTIFY_COOLDOWN_MIN, SLACK_WEBHOOK_URL, DISCORD_WEBHOOK_URL, EMAIL_ENABLED.

--------------------------------------------------------------------
Security & Secrets
--------------------------------------------------------------------
- `.gitignore` protects `.env`, `state/*.json`, `.direnv/`, `.shuttle/`, caches, etc.
- Use Shuttle SecretStore or local `.env` (never commit real keys).
- `.gitattributes` normalizes LF/CRLF; PowerShell scripts kept CRLF.

--------------------------------------------------------------------
License & data note
--------------------------------------------------------------------
- Code: MIT.
- Sentiment lexicon: custom, inspired by financial research, but independent.

--------------------------------------------------------------------
Contributing
--------------------------------------------------------------------
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.
