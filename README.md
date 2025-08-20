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
**Contributions, ideas, and comments are welcome.**
