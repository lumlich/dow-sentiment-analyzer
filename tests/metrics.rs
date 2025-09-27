// src/metrics.rs
//! Minimal Prometheus exporter wiring used by the API to expose `/metrics`.
//!
//! - `PROM` is a global handle to the Prometheus exporter (set during app boot).
//! - `Metrics` provides a tiny facade used by the binary to merge a `/metrics` route.

use axum::{routing::get, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use once_cell::sync::OnceCell;

/// Global Prometheus handle; set by the API initialization.
pub static PROM: OnceCell<PrometheusHandle> = OnceCell::new();

/// Small facade used by the binary.
pub struct Metrics;

impl Metrics {
    /// No-op init (recorder is typically installed in `api` during boot).
    #[allow(clippy::unused_self)]
    pub fn init(_ttl_ms: u64) -> Self {
        Self
    }

    /// Router that serves `/metrics`. If no exporter is installed, returns a benign string.
    pub fn router(&self) -> Router {
        Router::new().route(
            "/metrics",
            get(|| async {
                PROM.get()
                    .map(|h| h.render())
                    .unwrap_or_else(|| "metrics unavailable".to_string())
            }),
        )
    }
}

/// Convenience free function (not used by the binary, but handy in tests if needed).
pub fn router() -> Router {
    Metrics::init(0).router()
}
