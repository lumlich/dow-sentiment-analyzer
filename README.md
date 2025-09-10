# dow-sentiment-analyzer

[![Build status](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml/badge.svg)](https://github.com/lumlich/dow-sentiment-analyzer/actions)
[![Security audit](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml/badge.svg)](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml)

A sentiment analysis and decision engine for Dow Jones futures, built with Rust, Axum, and Tokio.

It processes short texts (e.g., statements by Trump, the Fed, Yellen, Reuters, etc.), scores sentiment with a small lexicon + negation handling, applies per-source weights, and produces a transparent BUY / HOLD / SELL decision with confidence and human-readable reasons.

---

## Features
- Lexicon-based sentiment with simple negation (e.g., “not good”).
- Configurable source weights via `source_weights.json` (+ aliases).
- Disruption detection (source strength × sentiment × recency).
- Confidence calibration with recent volume context (last 10 minutes).
- Rolling metrics (48h average & count) and in-memory decision history.
- Clean JSON API + debug endpoints.
- Slack + Discord webhook notifications (configurable).
- **Optional AI integration with caching and daily call limits.**

---

## Quickstart (local dev with Shuttle)
```bash
# clone and enter
git clone https://github.com/lumlich/dow-sentiment-analyzer.git
cd dow-sentiment-analyzer

# run tests & lints
cargo fmt
cargo clippy -- -D warnings
cargo test

# run locally (Shuttle dev runtime) – use this (do not use `cargo run`)
cargo shuttle run
```
> **Note:** The service runs under Shuttle's local runtime. Use `cargo shuttle run` instead of `cargo run`.

---

## Usage (API examples)

### GET /health  (root)
```bash
curl -s http://localhost:8000/health
```
Response:
```
OK
```

### GET /api/ping
```bash
curl -s http://localhost:8000/api/ping
```
Response:
```
pong
```

### POST /api/analyze
```bash
curl -s -X POST http://localhost:8000/api/analyze \
  -H "Content-Type: application/json" \
  -d '{"text":"Fed signals a cautious path to rate cuts this year.","source":"Fed"}'
```

### POST /api/batch
```bash
curl -s -X POST http://localhost:8000/api/batch \
  -H "Content-Type: application/json" \
  -d '[{"id":"a1","text":"Trump says Dow will soar.","source":"Trump"},
       {"id":"b2","text":"Reuters: unexpected slowdown in manufacturing.","source":"Reuters"}]'
```

### POST /api/decide
```bash
curl -s -X POST http://localhost:8000/api/decide \
  -H "Content-Type: application/json" \
  -d '[{"source":"Reuters","text":"ISM manufacturing dips below 50; the Dow slips."}]'
```
Response (example):
```json
{
  "decision": "SELL",
  "confidence": 0.68,
  "reasons": [
    "macro+hard combo matched",
    "Relevance gate passed with score 0.47"
  ]
}
```
If irrelevant (e.g., DJI drones):
```json
{
  "decision": "NEUTRAL",
  "reasons": ["neutralized: below relevance threshold"]
}
```

> **Windows / PowerShell tip:** `curl` is an alias for `Invoke-WebRequest`. Use either `curl.exe` (actual curl) **or** PowerShell cmdlets:
> ```powershell
> $body = '[{"source":"Fed","text":"Powell hints at uncertainty"}]'
> Invoke-WebRequest -Method POST -Uri "http://127.0.0.1:8000/api/decide" `
>   -ContentType "application/json" -Body $body -UseBasicParsing
> ```

---

## Development — Common tasks

We use cargo aliases (see `.cargo/config.toml`) for convenience:

- `cargo t` → run all fast unit tests  
- `cargo tu` → run unit tests only in this crate  
- `cargo ts` → run synthetic suite (marked `#[ignore]`)  
- `cargo cf` → check formatting  
- `cargo cl` → run Clippy with `-D warnings`  

---

## Strict Metrics Test (feature-gated)

A stricter ingest metrics test is available behind an optional Cargo feature. By default, it **does not** compile nor run so your local and CI suites stay green.

- Run default suite:
```bash
cargo test
```

- Run with strict metrics enabled:
```bash
cargo test --features strict-metrics
```

CI tip: you can add an optional job in your workflow to run the strict test without blocking the main CI.

---

## Relevance Gate

### What it does
Before sentiment, every input is scored for *market relevance* in `[0.0, 1.0]`.  
If `score < RELEVANCE_THRESHOLD`, the request is neutralized and the decision returns a neutral outcome with an explanatory reason.

### How it scores (precision-first)
- **Anchors** — strong patterns like `djia|dow jones|the dow` or `powell` near `fed|fomc|rates?`.  
- **Blockers** — exclude false positives (`dji drones`, `dow inc`).  
- **Proximity rules** — `near { pattern, window }` for contextual matches.  
- **Combos** — pass conditions, e.g. need both `macro` and `hard`.  
- **Weights** — category weights (e.g., `hard=3, macro=2, semi=2, soft=1`) combine into the score.

### Config schema (excerpt)
```toml
[weights]
hard = 3
macro = 2
semi = 2
soft = 1
verb = 1

[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "(?i)\\b(djia|dow jones|the dow)\\b"

[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\\bpowell\\b"
near = { pattern = "(?i)\\b(fed|fomc|rates?)\\b", window = 6 }

[[blockers]]
id = "dji_drones"
pattern = "(?i)\\bdji\\b"
near = { pattern = "(?i)\\b(drone|mavic|gimbal)\\b", window = 4 }

[[combos.pass_any]]
need = ["macro","hard"]
```

### Environment variables
| Variable                 | Default                 | Meaning                                         |
|--------------------------|-------------------------|-------------------------------------------------|
| `RELEVANCE_CONFIG_PATH`  | `config/relevance.toml` | Where to load the config                        |
| `RELEVANCE_THRESHOLD`    | `0.30`                  | Cutoff in `[0.0,1.0]`; below → neutralize       |
| `RELEVANCE_HOT_RELOAD=1` | off                     | Hot reload config in dev mode                   |
| `RELEVANCE_DEV_LOG=1`    | off                     | Dev logs with anonymized IDs                    |

---

## Phase 4 – AI Integration (optional)
[... unchanged in this snippet ...]

---

## Notifications (Phase 5)

### What gets notified
- **Decision changes** (`BUY ↔ SELL`, `HOLD` transitions) are the trigger.
- **Antiflutter** (cooldown) prevents spam during short-term oscillations.  
  First alert after a quiet period always passes; inside cooldown, alerts are suppressed.

### Channels
- **Slack** via webhook (`SLACK_WEBHOOK_URL`)
- **Discord** via webhook (`DISCORD_WEBHOOK_URL`)
- **Email** (optional) gated by `EMAIL_ENABLED` (module present; real delivery optional)

### Change Detector
The change detector polls your decision endpoint and emits alerts when a disruptive change is observed **and** antiflutter allows it.

- Code: `src/change_detector.rs`
- State persistence: `state/last_decision.json`

**Background loop**
```rust
dow_sentiment_analyzer::change_detector::run_change_detector_loop().await?;
```

**Environment**
| Variable               | Default                            | Purpose                          |
|------------------------|------------------------------------|----------------------------------|
| `DECIDE_URL`           | `http://127.0.0.1:8000/api/decide` | Endpoint to poll for decisions   |
| `NOTIFY_INTERVAL_SECS` | `15`                               | Polling interval (seconds)       |
| `NOTIFY_COOLDOWN_MIN`  | `180`                              | Antiflutter cooldown in minutes  |
| `SLACK_WEBHOOK_URL`    | *(unset)*                          | Slack channel incoming webhook   |
| `DISCORD_WEBHOOK_URL`  | *(unset)*                          | Discord channel incoming webhook |
| `EMAIL_ENABLED`        | `false`                            | Enable email notifications       |
| `APP_PUBLIC_URL`       | `https://example.com`              | Link included in messages        |

> **Windows / PowerShell (examples):**
> ```powershell
> $env:DECIDE_URL = "http://127.0.0.1:8000/api/decide"
> $env:NOTIFY_INTERVAL_SECS = "15"
> $env:NOTIFY_COOLDOWN_MIN  = "180"
> $env:SLACK_WEBHOOK_URL    = "XXXXXXXXXX"
> $env:DISCORD_WEBHOOK_URL  = "XXXXXXXXXX"
> cargo shuttle run
> ```

### Antiflutter
Module: `src/notify/antiflutter.rs`  
- Cooldown-based suppression; first alert always passes.  
- Unified `DecisionKind` (`BUY/SELL/HOLD/TEST`) lives in `src/notify/mod.rs`.

### Test Scenarios (Phase 5)
Standalone tests validate antiflutter behavior and change detection logic.

Run:
```bash
cargo test --tests
```

They cover:
- First decision → sends exactly once.
- Quick oscillation inside cooldown → suppressed.
- After cooldown → next change is sent.

## Small Env Snippets (Phase 5 follow‑up)

For local tinkering scripts/crons:
```bash
export DECIDE_ENDPOINT="http://127.0.0.1:8000/api/decide"
export CHECK_INTERVAL_SECS="15"
```

On Windows PowerShell:
```powershell
$env:DECIDE_ENDPOINT = "http://127.0.0.1:8000/api/decide"
$env:CHECK_INTERVAL_SECS = "15"
```

---

## License & data note
- Code: MIT.  
- Sentiment lexicon: custom, inspired by financial research, but independent.  

---

## Contributing
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.
