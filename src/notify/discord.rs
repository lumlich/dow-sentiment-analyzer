use anyhow::{Context, Result};
use reqwest::Client;

use super::{NotificationEvent, Notifier};

pub struct DiscordNotifier {
    webhook_url: Option<String>,
    client: Client,
}

impl DiscordNotifier {
    pub fn from_env() -> Self {
        Self {
            webhook_url: std::env::var("DISCORD_WEBHOOK_URL").ok(),
            client: Client::new(),
        }
    }

    /// Optional builder for tests/tools
    pub fn new(url: String) -> Self {
        Self {
            webhook_url: Some(url),
            client: Client::new(),
        }
    }

    pub fn with_timeout(self, _secs: u64) -> Self {
        self
    }

    pub fn with_retries(self, _n: u8) -> Self {
        self
    }
}

#[async_trait::async_trait]
impl Notifier for DiscordNotifier {
    async fn send(&self, ev: &NotificationEvent) -> Result<()> {
        let Some(url) = &self.webhook_url else {
            tracing::debug!("Discord disabled (no DISCORD_WEBHOOK_URL)");
            return Ok(());
        };

        let content = format!(
            "**DJI alert:** **{:?}** ({:.2})\nReason: {}\n{}",
            ev.decision,
            ev.confidence,
            ev.reasons.first().cloned().unwrap_or_default(),
            ev.ts.to_rfc3339()
        );
        let body = serde_json::json!({ "content": content });

        self.client
            .post(url)
            .json(&body)
            .send()
            .await
            .context("discord post")?
            .error_for_status()
            .context("discord non-2xx")?;
        Ok(())
    }
}
