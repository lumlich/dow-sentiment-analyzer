# dow-sentiment-analyzer

[![CI](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml/badge.svg)](https://github.com/lumlich/dow-sentiment-analyzer/actions/workflows/ci.yml)

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
curl -s -X POST http://localhost:8000/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "text": "Fed signals a cautious path to rate cuts this year.",
    "source": "Fed",
    "timestamp": "2025-08-18T12:45:00Z"
  }'
```
Response (example):
```json
{
  "score": 2,
  "tokens_count": 9,
  "sentiment": "positive",
  "decision": "BUY",
  "confidence": 0.72,
  "reasons": [
    "Positive tokens outweigh negatives",
    "High source weight for 'Fed'",
    "Recent statement increases effect"
  ],
  "meta": {
    "source": "Fed",
    "source_weight": 1.6,
    "negated_tokens": [],
    "disruption": 0.58,
    "timestamp": "2025-08-18T12:45:00Z"
  }
}
```

### POST /batch
```bash
curl -s -X POST http://localhost:8000/batch \
  -H "Content-Type: application/json" \
  -d '{
    "items": [
      {
        "id": "a1",
        "text": "Trump says Dow will soar.",
        "source": "Trump",
        "timestamp": "2025-08-18T10:05:00Z"
      },
      {
        "id": "b2",
        "text": "Reuters: unexpected slowdown in manufacturing.",
        "source": "Reuters",
        "timestamp": "2025-08-18T10:10:00Z"
      }
    ]
  }'
```
Response (example):
```json
{
  "results": [
    {
      "id": "a1",
      "score": 1,
      "sentiment": "positive",
      "decision": "BUY",
      "confidence": 0.63,
      "meta": { "source": "Trump", "source_weight": 1.4, "disruption": 0.44 }
    },
    {
      "id": "b2",
      "score": -2,
      "sentiment": "negative",
      "decision": "SELL",
      "confidence": 0.69,
      "meta": { "source": "Reuters", "source_weight": 0.9, "disruption": 0.51 }
    }
  ]
}
```

---

## Development — Common tasks

We use cargo aliases (see `.cargo/config.toml`) for convenience:

- `cargo t` → run all fast unit tests across workspace  
- `cargo tu` → run unit tests only in this crate (`dow-sentiment-analyzer`)  
- `cargo ts` → run synthetic suite (marked `#[ignore]`)  
- `cargo cf` → check formatting (`cargo fmt --check`)  
- `cargo cl` → run Clippy with `-D warnings`  

---

## Project Roadmap
The development is structured into 6 phases (Core logic, Frontend, Contextual rules, AI integration, Notifications, Stability & Ops).  
See [Milestones](https://github.com/lumlich/dow-sentiment-analyzer/milestones) for detailed progress.

---

## License & data note
- Code: MIT.  
- The included sentiment lexicon is a **custom list**, inspired by financial sentiment research (e.g., Loughran–McDonald), but **heavily adapted and extended** with our own terms.  
- It should be treated as an independent resource, not a redistribution of the original L-M dataset.  

---

## Contributing
Open an Issue with `feat:` or `bug:` prefix; PRs welcome.  
See [Issues](../../issues) and [Milestones](../../milestones).

---

### Relevance Gate

**What it does.** Before sentiment, every input is scored for *market relevance* in `[0.0, 1.0]`.  
If `score < RELEVANCE_THRESHOLD`, the request is **neutralized** (treated as irrelevant noise) and the decision pipeline returns a neutral outcome with an explanatory reason.

**How it scores (precision-first):**
- **Anchors** — strong patterns like `djia|dow jones|the dow|dow` or `powell` near `fed|fomc|rates?`.
- **Blockers** — exclude common false positives (e.g., `dji drones`, `dow inc`).
- **Proximity rules** — `near { pattern, window }` to require context proximity.
- **Combos** — pass conditions, e.g. need both `macro` **and** `hard` categories.
- **Weights** — category weights (e.g., `hard=3, macro=2, soft=1`) combine into the final score.

**Config schema (TOML, excerpt):**
```toml
[weights]
hard = 3
macro = 2
soft = 1

[blockers]
patterns = [
  "\\b(dji drones?|mavic)\\b",
  "\\bdow inc\\b",
]

[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "\\b(djia|dow jones|the dow|dow)\\b"

[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "\\bpowell\\b"
near = { pattern = "\\b(fed|fomc|rates?)\\b", window = 6 }

[[combos.pass_any]]
need = ["macro","hard"]
```

**Hot-reload (dev only).** When running locally with `SHUTTLE_ENV=local`, changes to `RELEVANCE_CONFIG_PATH` are reloaded without restart.

**Environment**
| Variable                 | Default                 | Meaning                                         |
|-------------------------|-------------------------|-------------------------------------------------|
| `RELEVANCE_CONFIG_PATH` | `config/relevance.toml` | Where to load the anchors/blockers config from. |
| `RELEVANCE_THRESHOLD`   | `0.5`                   | Score cutoff in `[0.0,1.0]`; below → neutralize.|

Examples:
```bash
RELEVANCE_THRESHOLD=0.65 cargo run
RELEVANCE_CONFIG_PATH=.local/relevance.dev.toml cargo test -p dow-sentiment-analyzer
```

**API example — POST /decide (relevance gate in action)**
```bash
curl -s -X POST http://localhost:8000/decide \
  -H "Content-Type: application/json" \
  -d '{ "text": "Powell signals patience; FOMC holds rates." }'
```
Sample response:
```json
{
  "decision": "HOLD",
  "relevance": {
    "score": 0.74,
    "matched": ["powell_near_fed_rates","djia_core_names"],
    "reasons": ["combo macro+hard matched"]
  }
}
```
If the input is off-topic (e.g., DJI drones), you’ll see:
```json
{
  "decision": "NEUTRAL",
  "relevance": {
    "score": 0.00,
    "reasons": ["neutralized: below relevance threshold"]
  }
}
```

---

**Contributions, ideas, and comments are welcome.**
