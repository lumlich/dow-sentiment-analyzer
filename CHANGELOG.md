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

## [v0.1.0] - 2025-08-20
### Added
- Core sentiment scoring logic.
- Axum API endpoints: `/health`, `/analyze`, `/batch`, `/decide`.
- Rolling stats & decision history internals.
- Debug endpoints (runtime-gated for local): `/history`, `/stats`.
- CI (fmt, clippy, tests, build) via GitHub Actions.
- Weekly Dependabot for Cargo & Actions (labels + target branch).
- Issue & PR templates, CODEOWNERS.
- Dockerfile & `.dockerignore`.

### Changed
- Source weight loading from `source_weights.json`.

### Fixed
- Clippy warnings and formatting to satisfy CI.