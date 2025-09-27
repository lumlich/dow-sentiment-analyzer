//! Crate root – zveřejní moduly a reexportuje klíčové symboly očekávané testy/binárkami.
//! NIC z tvých existujících modulů nemažeme; jen je připojíme do kořene.

#![forbid(unsafe_code)]

pub mod analyze;
pub mod api;
pub mod ingest;
pub mod last;
pub mod notify;
pub mod relevance;
pub mod versions;

// Přidáno: metriky (Prometheus /metrics)
pub mod metrics;

// Moduly, které používá src/api.rs — přidej je do kořene, aby byly dostupné přes `crate::…`.
pub mod ai_adapter;
pub mod decision;
pub mod disruption;
pub mod engine;
pub mod history;
pub mod rolling;
pub mod sentiment;
pub mod source_weights;

// ——— Reexports, které očekávají testy a binárky ———
pub use crate::api::router as app;
pub use crate::notify::{DecisionKind, NotificationEvent, NotifierMux};
pub use crate::relevance::Relevance;
