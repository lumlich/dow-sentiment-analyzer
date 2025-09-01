// src/ai_bootstrap.rs
use crate::analyze::ai_adapter::{build_ai_client, AiClient};
use crate::config::ai::AiConfig;
use std::sync::Arc;
use tracing::{info, warn};

pub struct AiRuntime {
    pub cfg: AiConfig,
    pub client: Arc<dyn AiClient>,
}

impl AiRuntime {
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
        let cfg = AiConfig::load_from_file(path)?;
        // Safe diagnostics: only provider + enabled + key length
        info!(
            "AI cfg loaded: provider={}, enabled={}, key_len={}",
            cfg.provider,
            cfg.enabled,
            cfg.api_key.len()
        );
        // NOTE: build_ai_client() now takes no arguments (reads config internally)
        let client = build_ai_client();
        Ok(Self { cfg, client })
    }

    pub async fn quick_probe(&self) {
        if !self.cfg.enabled {
            warn!("AI quick_probe skipped: AI is disabled in config");
            return;
        }
        let sample =
            "Fed signals possible rate cut if labor market cools further, weighing on Dow futures.";
        let out = self.client.analyze(sample).await;
        info!("AI quick_probe => {:?}", out);
    }
}
