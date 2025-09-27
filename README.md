dow-sentiment-analyzer — README
================================

[Build status] https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml
[Security audit] https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml

A sentiment analysis and decision engine for Dow Jones futures, built with Rust, Axum, and Tokio.

It processes short texts (e.g., statements by the Fed, the White House, Reuters, etc.),
scores sentiment with a small lexicon + negation handling, applies per-source weights,
and produces a transparent BUY / HOLD / SELL decision with confidence and human‑readable reasons.

--------------------------------------------------------------------
Features
--------------------------------------------------------------------
• Lexicon-based sentiment with simple negation (e.g., “not good”).
• Configurable source weights via `source_weights.json` (+ aliases).
• Disruption detection (source strength × sentiment × recency).
• Confidence calibration with recent volume context (last 10 minutes).
• Rolling metrics (48h average & count) and in-memory decision history.
• Clean JSON API + debug endpoints.
• Slack + Discord webhook notifications (configurable).
• Optional AI integration with caching and daily call limits.

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

# run locally (Shuttle dev runtime) – use this (do NOT use `cargo run`)
cargo shuttle run

NOTE: The service runs under Shuttle's local runtime. Use `cargo shuttle run` instead of `cargo run`.

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
       -d '[{"id":"a1","text":"Powell says outlook is mixed.","source":"Fed"},
             {"id":"b2","text":"Reuters: slowdown in manufacturing.","source":"Reuters"}]'

POST /api/decide
  curl -s -X POST http://localhost:8000/api/decide \
       -H "Content-Type: application/json" \
       -d '[{"source":"Reuters","text":"ISM manufacturing dips below 50; the Dow slips."}]'

Example response:
{
  "decision": "SELL",
  "confidence": 0.68,
  "reasons": ["macro+hard combo matched", "Relevance gate passed with score 0.47"]
}

If irrelevant (e.g., DJI drones):
{
  "decision": "NEUTRAL",
  "reasons": ["neutralized: below relevance threshold"]
}

Windows / PowerShell tip:
  `curl` is an alias for Invoke-WebRequest. Use either `curl.exe` (the real curl) or PowerShell:
    $body = '[{"source":"Fed","text":"Powell hints at uncertainty"}]'
    Invoke-WebRequest -Method POST -Uri "http://127.0.0.1:8000/api/decide" `
      -ContentType "application/json" -Body $body -UseBasicParsing

--------------------------------------------------------------------
Development — Common tasks
--------------------------------------------------------------------
We use cargo aliases (see `.cargo/config.toml`) for convenience:

• cargo t   → run all fast unit tests
• cargo tu  → run unit tests only in this crate
• cargo ts  → run synthetic suite (marked #[ignore])
• cargo cf  → check formatting
• cargo cl  → run Clippy with `-D warnings`

--------------------------------------------------------------------
Phase 6 — Ingest (HTTP + Fixtures)
--------------------------------------------------------------------
Features:
  ingest-fixtures  Build providers from local XML fixtures (no network)         [default: on]
  ingest-http      Enable HTTP-backed providers (from_url, fetch_latest)        [opt-in]
  strict-metrics   Compile strict ingest metrics test                            [opt-in]
  strict-e2e       Compile a strict E2E smoke test for /decide                   [opt-in]

Defaults remain unchanged: `default = ["ingest-fixtures"]`. CI does NOT make network calls.

HTTP ingest (opt-in; example):
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
  • Logs warn! with provider name
  • Increments `ingest_provider_errors_total`

End-to-end (fixtures → analyze → verdict):
  cargo test --test ingest_e2e_decision

Strict E2E for /decide (opt-in):
  cargo test --features "strict-e2e" --test ingest_e2e -- --nocapture

Build & lint with HTTP enabled (compilation-only):
  cargo check  --features ingest-http
  cargo clippy --features ingest-http -- -D warnings
  cargo test   --no-run --features ingest-http

Runtime backup smoke (optional):
  A) Unit
     cargo test --test backup_cron -- --nocapture

  B) Dev logs (Windows PowerShell)
     $env:RUST_LOG = "debug"
     cargo shuttle run
     # Check logs for: "backup sink stored <n> files"

--------------------------------------------------------------------
Strict Metrics Test (feature-gated)
--------------------------------------------------------------------
A stricter ingest metrics test is available behind an optional Cargo feature.
By default, it does not compile nor run.

  cargo test                          # default suite
  cargo test --features strict-metrics  # enable the strict test

--------------------------------------------------------------------
Relevance Gate
--------------------------------------------------------------------
What it does
  Before sentiment, every input is scored for market relevance in [0.0, 1.0].
  If score < RELEVANCE_THRESHOLD, the request is neutralized and the decision
  returns a neutral outcome with an explanatory reason.

How it scores (precision-first)
  • Anchors — strong patterns like djia|dow jones|the dow or powell near fed|fomc|rates?
  • Blockers — exclude false positives (dji drones, dow inc).
  • Proximity rules — `near { pattern, window }` for contextual matches.
  • Combos — pass conditions, e.g. need both `macro` and `hard`.
  • Weights — category weights (hard=3, macro=2, semi=2, soft=1) combine into the score.

Environment
  RELEVANCE_CONFIG_PATH   (default: config/relevance.toml)
  RELEVANCE_THRESHOLD     (default: 0.30)
  RELEVANCE_HOT_RELOAD=1  (dev-only hot reload)
  RELEVANCE_DEV_LOG=1     (dev logs with anonymized IDs)

--------------------------------------------------------------------
Notifications (Phase 5)
--------------------------------------------------------------------
What gets notified
  • Decision changes (BUY ↔ SELL, HOLD transitions). 
  • Antiflutter cooldown prevents spam during oscillations.

Channels
  • Slack via webhook (SLACK_WEBHOOK_URL)
  • Discord via webhook (DISCORD_WEBHOOK_URL)
  • Email (optional) gated by EMAIL_ENABLED

Change Detector
  Polls your decision endpoint and emits alerts when a disruptive change is observed
  and antiflutter allows it.
  State persistence: state/last_decision.json

Environment
  DECIDE_URL            (default: http://127.0.0.1:8000/api/decide)
  NOTIFY_INTERVAL_SECS  (default: 15)
  NOTIFY_COOLDOWN_MIN   (default: 180)
  SLACK_WEBHOOK_URL     (unset by default)
  DISCORD_WEBHOOK_URL   (unset by default)
  EMAIL_ENABLED         (default: false)
  APP_PUBLIC_URL        (e.g., https://example.com)

Windows PowerShell (example):
  $env:DECIDE_URL = "http://127.0.0.1:8000/api/decide"
  $env:NOTIFY_INTERVAL_SECS = "15"
  $env:NOTIFY_COOLDOWN_MIN  = "180"
  $env:SLACK_WEBHOOK_URL    = "XXXXXXXXXX"
  $env:DISCORD_WEBHOOK_URL  = "XXXXXXXXXX"
  cargo shuttle run

--------------------------------------------------------------------
Small Env Snippets (helpers)
--------------------------------------------------------------------
Bash:
  export DECIDE_ENDPOINT="http://127.0.0.1:8000/api/decide"
  export CHECK_INTERVAL_SECS="15"

PowerShell:
  $env:DECIDE_ENDPOINT = "http://127.0.0.1:8000/api/decide"
  $env:CHECK_INTERVAL_SECS = "15"

--------------------------------------------------------------------
License & data note
--------------------------------------------------------------------
• Code: MIT.
• Sentiment lexicon: custom, inspired by financial research, but independent.

--------------------------------------------------------------------
Contributing
--------------------------------------------------------------------
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.
