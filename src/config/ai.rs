// src/config/ai.rs
use serde::{Deserialize, Serialize};
use std::{env, fs, path::Path};

fn default_band_min() -> f32 {
    0.40
}
fn default_band_max() -> f32 {
    0.60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    /// "openai" | "claude" (case-insensitive)
    pub provider: String,
    pub daily_limit: u32,
    /// "ENV" means: read from OPENAI_API_KEY / CLAUDE_API_KEY (by provider)
    pub api_key: String,
    /// Confidence band to trigger AI hint (inclusive). Defaults 0.40–0.60.
    #[serde(default = "default_band_min")]
    pub band_min: f32,
    #[serde(default = "default_band_max")]
    pub band_max: f32,
}

impl AiConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let data = fs::read_to_string(path)?;
        let mut cfg: AiConfig = serde_json::from_str(&data)?;

        // Normalize provider
        cfg.provider = cfg.provider.to_lowercase();

        // Resolve api key if "ENV"
        if cfg.api_key.trim().eq_ignore_ascii_case("env") {
            cfg.api_key = match cfg.provider.as_str() {
                "openai" => env::var("OPENAI_API_KEY")
                    .map_err(|_| anyhow::anyhow!("Missing OPENAI_API_KEY env var"))?,
                "claude" => env::var("CLAUDE_API_KEY")
                    .map_err(|_| anyhow::anyhow!("Missing CLAUDE_API_KEY env var"))?,
                other => anyhow::bail!("Unsupported provider in config: {other}"),
            };
        }

        // Sanitize band
        if !(0.0..=1.0).contains(&cfg.band_min) {
            cfg.band_min = default_band_min();
        }
        if !(0.0..=1.0).contains(&cfg.band_max) {
            cfg.band_max = default_band_max();
        }
        if cfg.band_min > cfg.band_max {
            // swap to keep a valid interval
            std::mem::swap(&mut cfg.band_min, &mut cfg.band_max);
        }

        Ok(cfg)
    }
}
