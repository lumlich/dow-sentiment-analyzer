//! Debug endpoints for Phase 3: inspect weights/rules and preview decisions.
//! Mount with e.g. `app.merge(analyze::debug::router())` in dev only.

use serde::Serialize;
use shuttle_axum::axum::{extract::Query, routing::get, Json, Router};

use super::{
    analyze_and_decide_with_signals, rules::HotReloadRules, scoring::ScoreInputs,
    weights::HotReloadWeights,
};

#[derive(Debug, Serialize)]
pub struct WeightsOut {
    pub w_source: f32,
    pub w_strength: f32,
    pub w_recency: f32,
}

#[derive(Debug, Serialize)]
pub struct RulesOut {
    pub count: usize,
    pub names: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PreviewOut {
    pub action: String,
    pub confidence: f32,
    pub reasons: Vec<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/debug/weights", get(get_weights))
        .route("/debug/rules", get(get_rules))
        .route("/debug/decide_preview", get(get_decide_preview))
}

async fn get_weights() -> Json<WeightsOut> {
    let hot = HotReloadWeights::new(None);
    let w = hot.current();
    Json(WeightsOut {
        w_source: w.w_source,
        w_strength: w.w_strength,
        w_recency: w.w_recency,
    })
}

async fn get_rules() -> Json<RulesOut> {
    let hot = HotReloadRules::new(None);
    let rs = hot.current();
    Json(RulesOut {
        count: rs.rules.len(),
        names: rs
            .rules
            .iter()
            .map(|r| r.name.clone().unwrap_or_else(|| "<unnamed>".to_string()))
            .collect(),
    })
}

/// GET /debug/decide_preview?text=...&source=0.5&strength=0.5&recency=0.5
async fn get_decide_preview(
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Json<PreviewOut> {
    let text = q.get("text").cloned().unwrap_or_default();
    let pf = |k: &str, d: f32| {
        q.get(k)
            .and_then(|s| s.parse().ok())
            .unwrap_or(d)
            .clamp(0.0, 1.0)
    };

    let inputs = ScoreInputs::new(pf("source", 0.5), pf("strength", 0.5), pf("recency", 0.5));
    let res = analyze_and_decide_with_signals(&text, inputs);

    Json(PreviewOut {
        action: res.action,
        confidence: res.confidence,
        reasons: res.reasons,
    })
}
