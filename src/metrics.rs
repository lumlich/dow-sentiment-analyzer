// src/metrics.rs
//! Minimal Prometheus exporter wiring used by the API to expose `/metrics`.
//!
//! - `PROM` je globální handle na Prometheus exporter (naplní ho inicializace v `api.rs`).
//! - `router()` vrací malý Axum router se `/metrics`, který bezpečně funguje
//!   i když exporter není nainstalovaný (vrátí benigní text).

use axum::{routing::get, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use once_cell::sync::OnceCell;

/// Globální Prometheus handle, nastavovaný při bootu (viz `api.rs`).
pub static PROM: OnceCell<PrometheusHandle> = OnceCell::new();

/// Router s `/metrics`. Když není exporter nainstalovaný,
/// vrací harmless text "metrics unavailable".
pub fn router() -> Router {
    Router::new().route(
        "/metrics",
        get(|| async {
            PROM.get()
                .map(|h| h.render())
                .unwrap_or_else(|| "metrics unavailable".to_string())
        }),
    )
}
