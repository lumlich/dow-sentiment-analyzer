# dow-sentiment-analyzer

[![Build status](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml/badge.svg)](https://github.com/lumlich/dow-sentiment-analyzer/actions)

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
curl -s -X POST http://localhost:8000/api/analyze   -H "Content-Type: application/json"   -d '{"text":"Fed signals a cautious path to rate cuts this year.","source":"Fed"}'
```

### POST /api/batch
```bash
curl -s -X POST http://localhost:8000/api/batch   -H "Content-Type: application/json"   -d '[{"id":"a1","text":"Trump says Dow will soar.","source":"Trump"},
       {"id":"b2","text":"Reuters: unexpected slowdown in manufacturing.","source":"Reuters"}]'
```

### POST /api/decide
```bash
curl -s -X POST http://localhost:8000/api/decide   -H "Content-Type: application/json"   -d '[{"source":"Reuters","text":"ISM manufacturing dips below 50; the Dow slips."}]'
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
pattern = "(?i)\b(djia|dow jones|the dow)\b"

[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\bpowell\b"
near = { pattern = "(?i)\b(fed|fomc|rates?)\b", window = 6 }

[[blockers]]
id = "dji_drones"
pattern = "(?i)\bdji\b"
near = { pattern = "(?i)\b(drone|mavic|gimbal)\b", window = 4 }

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

## Phase 2 – Frontend UI
[... unchanged ...]

---

## Phase 3 – Contextual rules (AI-ready design)
[... unchanged ...]

---

## Phase 4 – AI Integration (optional)

### Overview
- AI is only called **sometimes**, on borderline or ambiguous decisions.  
- Responses include both **headers** and **JSON fields** indicating AI usage.  
- AI calls are cached per input hash in `cache/ai/` and limited daily.

### Config
File: `config/ai.json`
```json
{
  "enabled": true,
  "provider": "openai",
  "daily_limit": 50
}
```

Environment variables:
| Variable              | Default | Meaning                                |
|-----------------------|---------|----------------------------------------|
| `OPENAI_API_KEY`      | none    | API key for provider (e.g. OpenAI)     |
| `AI_SOURCES`          | all     | Sources allowed for AI use             |
| `AI_ONLY_TOP_SOURCES` | true    | Restrict AI to top-weighted sources    |
| `AI_SCORE_BAND`       | 0.08    | Only borderline decisions trigger AI   |

### Response format
Headers:
- `X-AI-Used: 1|0` *(sometimes `yes|no` depending on build; this project uses `1|0`)*
- `X-AI-Reason: <short sanitized reason>` *(present only if AI contributed)*

JSON field `ai`:
```json
"ai": {
  "used": true,
  "reason": "AI short summary",
  "cache_hit": false,
  "limited": false
}
```

### Examples

#### Borderline case → AI call
```bash
curl -i -X POST http://localhost:8000/api/decide   -H "Content-Type: application/json"   -d '[{"source":"Fed","text":"Powell hints at uncertainty"}]'
```
Response headers (example):
```
X-AI-Used: 1
X-AI-Reason: borderline clarified by AI
```
Response JSON includes:
```json
"ai": { "used": true, "reason": "borderline clarified by AI", "cache_hit": false, "limited": false }
```

#### Cache hit
Send the **same** request again. The response JSON shows:
```json
"ai": { "used": true, "cache_hit": true }
```

#### Disabled AI
```bash
export AI_ENABLED=0
curl -i -X POST http://localhost:8000/api/decide   -H "Content-Type: application/json"   -d '[{"source":"Fed","text":"Powell says the Fed may cut rates; Dow Jones futures slip"}]'
```
Response headers:
```
X-AI-Used: 0
```
Response JSON:
```json
"ai": { "used": false, "limited": false }
```

> **Windows / PowerShell equivalents:**
> ```powershell
> $body = '[{"source":"Fed","text":"Powell hints at uncertainty"}]'
> Invoke-WebRequest -Method POST -Uri "http://127.0.0.1:8000/api/decide" `
>   -ContentType "application/json" -Body $body -UseBasicParsing | % Headers
> Invoke-RestMethod  -Method POST -Uri "http://127.0.0.1:8000/api/decide" `
>   -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 6
> ```
> If you hit `400 Bad Request` with `curl` in PowerShell, prefer `Invoke-WebRequest`/`Invoke-RestMethod` or use `curl.exe --data-binary`.

---

## AI integration tests (mock)

Run the local server and tests in mock mode:

```powershell
$env:AI_TEST_MODE = "mock"
$env:SHUTTLE_ENV = "local"
cargo shuttle run
```

In another terminal:

```powershell
$env:AI_TEST_MODE = "mock"
cargo test --test ai_integration -- --ignored --nocapture
```

> Full suite including ignored: `cargo test -- --include-ignored`

---

## Real AI calls locally (no commits, env-only)

```powershell
Remove-Item Env:AI_TEST_MODE -ErrorAction SilentlyContinue
$env:OPENAI_API_KEY = "XXXXXXXXXX"
$env:SHUTTLE_ENV   = "local"
cargo shuttle run
```

Verify headers:

```powershell
curl http://127.0.0.1:8000/api/decide `
  -Method POST `
  -Body '{ "text": "Fed hints at cuts; labor market cools" }' `
  -ContentType "application/json" -i
```

Cleanup:

```powershell
Remove-Item Env:OPENAI_API_KEY
```

---

## CI

- GH Actions runs `fmt`, `clippy -D warnings`, and fast tests (`cargo test`) on PRs and `main`.
- AI integration tests are excluded from CI by default (they require a running local server).

---

## License & data note
- Code: MIT.  
- Sentiment lexicon: custom, inspired by financial research, but independent.  

---

## Contributing
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.

---

## Notifications

- **Slack**: configurable webhook via `SLACK_WEBHOOK_URL`
- **Discord**: configurable webhook via `DISCORD_WEBHOOK_URL`

[![Security audit](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml/badge.svg)](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/audit.yml)
