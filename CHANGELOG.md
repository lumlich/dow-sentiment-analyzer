# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed
- Tests: upgraded to `rand = 0.9` and updated the synthetic suite (using `rand::prelude::IndexedRandom`, replaced `gen_bool` → `random_bool`, cleaned up imports).
- Improved determinism in the synthetic suite with seeded `StdRng`.

### Fixed
- Full test compatibility with `rand 0.9`; removed warnings and deprecated calls.

## [v0.3.0] - 2025-08-27
### Added
- Contextual rules (Phase 3):
  - NER + regex/keyword configs (`./config/*.json`) enrich reasons panel.
  - Reranking: latest relevant statement per source, earlier near-duplicates decayed.
  - Antispam: sliding-window filter for near-identical inputs (Levenshtein similarity).
  - Calibration: confidence influenced by `weights.json`, hot-reload supported.
- Synthetic integration suite (`tests/f3_synthetic.rs`) covering all contextual rules.

### Changed
- README updated with Phase 3 section.
- Documentation clarified around contextual rules.

### Fixed
- Passing synthetic suite: 5/5 tests green (NER, Rerank, Antispam, Calibration, Rules).

## [v0.2.1] - 2025-08-25
### Added
- Relevance gate: anchors, blockers, proximity, combos, and category weights.
- Hot-reload of relevance config in dev via `RELEVANCE_CONFIG_PATH`.
- Env-based tuning (`RELEVANCE_CONFIG_PATH`, `RELEVANCE_THRESHOLD`).
- E2E smoke test (`tests/e2e_smoke.rs`) confirming `/decide` pass/neutralize.
- Synthetic suite (`cargo ts`) for regression testing relevance.
- Dev tracing of neutralizations (anonymized, gated to `SHUTTLE_ENV=local`).

### Changed
- `/decide` endpoint now neutralizes inputs below threshold and returns explicit reasons.
- README expanded with Relevance Gate schema, config, and usage.

### Fixed
- Clippy warnings, formatting, and stable CI pipeline.

## [v0.1.0] - 2025-08-20
### Added
- Core lexicon-based sentiment scoring with negation handling.
- Configurable source weights (`source_weights.json` + aliases).
- Disruption detection (source × sentiment × recency).
- Rolling stats (48h average & count) and in-memory decision history.
- Decision engine producing BUY / HOLD / SELL with confidence & reasons.
- Axum API endpoints: `/health`, `/analyze`, `/batch`, `/decide`.
- Debug endpoints (runtime-gated via `SHUTTLE_ENV=local`): `/history`, `/rolling`, `/last-decision`, `/source-weight`.
- CI (fmt, clippy, tests, build) via GitHub Actions.
- Weekly Dependabot for Cargo & Actions (labels + target branch).
- Issue & PR templates, CODEOWNERS.
- Dockerfile & `.dockerignore`.

### Changed
- Source weight loading and runtime reload via `/admin/reload-source-weights`.

### Fixed
- Clippy warnings and formatting to satisfy CI.
