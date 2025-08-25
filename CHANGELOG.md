# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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