//! # MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.
//!
//! ## Endpoints
//! - `GET /health` — liveness check
//! - `POST /analyze` — analyze a single text
//! - `POST /batch` — analyze multiple texts
//! - `POST /decide` — decide BUY/SELL/HOLD with confidence
//! - `GET /debug/*` — diagnostics (rolling, history, source weights, last decision)
//! - `GET /admin/reload-source-weights` — reloads `source_weights.json`
//!
//! See `README.md` for quickstart and `docs/` for architecture notes.

mod api;
mod decision;
mod disruption;
mod engine;
mod history;
mod rolling;
mod sentiment;
mod source_weights;

use shuttle_axum::ShuttleAxum;

/// Application entrypoint for Shuttle runtime.
///
/// Initializes the Axum router from `api::create_router()` and hands it
/// off to Shuttle's deployment runtime.
#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    let router = api::create_router();
    Ok(router.into())
}
