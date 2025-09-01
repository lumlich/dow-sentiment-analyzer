//! Lightweight scoring helpers for Phase 3 calibration.
//!
//! `ScoreInputs` jsou tři normalizované signály v [0,1]:
//! - `source_score`   : kvalita/autorita zdroje
//! - `strength_score` : síla/entropie/konzistence signálu
//! - `recency_score`  : čerstvost/aktuálnost
//!
//! Base confidence = w_source*source + w_strength*strength + w_recency*recency
//! (normalizace a clamp do [0,1] je součástí výpočtu).

use super::Weights;

/// Normalized inputs in [0,1]. Keep it small and clear.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScoreInputs {
    pub source_score: f32,
    pub strength_score: f32,
    pub recency_score: f32,
}

impl ScoreInputs {
    /// Safe constructor with clamping.
    pub fn new(source: f32, strength: f32, recency: f32) -> Self {
        fn c(x: f32) -> f32 {
            x.clamp(0.0, 1.0)
        }
        Self {
            source_score: c(source),
            strength_score: c(strength),
            recency_score: c(recency),
        }
    }
}

/// Compute base confidence using calibrated Weights.
pub fn base_confidence(inputs: &ScoreInputs, w: &Weights) -> f32 {
    let raw = inputs.source_score * w.w_source
        + inputs.strength_score * w.w_strength
        + inputs.recency_score * w.w_recency;

    // Light normalization: divide by sum of weights if > 0, then clamp.
    let denom = (w.w_source + w.w_strength + w.w_recency).max(1e-6);
    (raw / denom).clamp(0.0, 1.0)
}
