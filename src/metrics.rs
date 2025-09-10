use axum::{routing::get, Router};
use metrics::gauge;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

pub struct Metrics {
    pub handle: PrometheusHandle,
}

impl Metrics {
    /// Initialize Prometheus recorder and expose a static gauge for the cache TTL.
    pub fn init(ttl_ms: u64) -> Self {
        // Use default buckets to avoid API differences across crate versions.
        let builder = PrometheusBuilder::new();

        let handle = builder
            .install_recorder()
            .expect("prometheus: install recorder");

        // Static gauge with current TTL (absolute TTL, no sliding refresh)
        gauge!("ai_decision_cache_ttl_ms").set(ttl_ms as f64);

        Self { handle }
    }

    /// Returns a router exposing `/metrics` with the Prometheus exposition format.
    pub fn router(&self) -> Router {
        let handle = self.handle.clone();
        Router::new().route(
            "/metrics",
            get(move || {
                let h = handle.clone();
                async move { h.render() }
            }),
        )
    }
}
