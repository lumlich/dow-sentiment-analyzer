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

# run locally (Shuttle dev runtime)
cargo shuttle run
```

---

## Usage (API examples)

### GET /health
```bash
curl -s http://localhost:8000/health
```
Response:
```
ok
```

### POST /analyze
```bash
curl -s -X POST http://localhost:8000/analyze   -H "Content-Type: application/json"   -d '{"text":"Fed signals a cautious path to rate cuts this year.","source":"Fed"}'
```

### POST /batch
```bash
curl -s -X POST http://localhost:8000/batch   -H "Content-Type: application/json"   -d '[{"id":"a1","text":"Trump says Dow will soar.","source":"Trump"},
       {"id":"b2","text":"Reuters: unexpected slowdown in manufacturing.","source":"Reuters"}]'
```

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
pattern = "(?i)(djia|dow jones|the dow)"

[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)powell"
near = { pattern = "(?i)(fed|fomc|rates?)", window = 6 }

[[blockers]]
id = "dji_drones"
pattern = "(?i)dji"
near = { pattern = "(?i)(drone|mavic|gimbal)", window = 4 }

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

### Example
```bash
curl -s -X POST http://localhost:8000/decide   -H "Content-Type: application/json"   -d '[{"source":"Reuters","text":"ISM manufacturing dips below 50; the Dow slips."}]'
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

---

## Phase 2 – Frontend UI

The project now includes a frontend served together with the backend (via Shuttle).  
The UI is designed as a single-page app that updates in real time.

### 🖥️ Dev quickstart
- `cd ui && npm install`
- `npm run dev` (served on Vite dev server, proxied to backend on port 8000)
- For Shuttle local backend: `cargo shuttle run`
- Open http://localhost:5173

### 🔌 API (dev)
- Frontend fetches from `/analyze` → proxied to backend `/decide`.
- Data format:
  ```json
  {
    "decision": "BUY | SELL | HOLD",
    "confidence": 0.83,
    "reasons": ["Trump statement...", "Fed speech...", "Market data..."],
    "contributors": ["Trump", "Fed"]
  }
  ```

### 📊 Panels
- **Verdict panel** – shows BUY/SELL/HOLD, color-coded, flashes on change.
- **Why panel** – top 3 reasons.
- **Evidence panel** – expandable accordion, shows detailed evidence (text + source + sentiment + timestamp).
- **Sentiment trend** – mini sparkline chart (last X minutes).

### ⚠️ Error handling
- If fetch fails, UI shows a fallback message and retries on next poll.
- Polling interval: 15s.

### 🏗️ Build (prod)
- `npm run build` → generates static assets in `/dist`
- Served by Shuttle as part of the Rust project.

### ✅ QA checklist
- [x] Verdict panel changes color and plays alert sound on decision change.
- [x] Evidence panel expands/collapses and shows reasons.
- [x] Sentiment trend updates.
- [x] UI loads successfully on Shuttle.

---

## Phase 3 – Contextual rules (AI-ready design)

**Goal:** Reduce false positives, improve explainability, prepare logic for later AI integration.

### Implemented
- **NER + Regex/Keyword Config** – enriches reasons from JSON configs in `./config/`.
- **Reranking** – keeps the latest relevant statement per source, decays earlier near-duplicates.
- **Antispam** – filters near-identical statements in a sliding time window.
- **Calibration** – confidence is dynamically influenced by `weights.json` (hot-reload).

### Tests
Synthetic integration suite lives in `tests/f3_synthetic.rs`.  
Run it with:
```bash
cargo test f3_ -- --nocapture
```
All five tests (NER, Rerank, Antispam, Calibration, Rules) currently pass ✅.

**Milestone:** Contextual rules are fully implemented, tested, and documented.

---

## License & data note
- Code: MIT.  
- Sentiment lexicon: custom, inspired by financial research, but independent.  

---

## Contributing
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.
