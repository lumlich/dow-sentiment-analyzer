use anyhow::{Context, Result};
use reqwest::Client;

use super::{NotificationEvent, Notifier};

pub struct SlackNotifier {
    webhook_url: Option<String>,
    client: Client,
}

impl SlackNotifier {
    pub fn from_env() -> Self {
        Self {
            webhook_url: std::env::var("SLACK_WEBHOOK_URL").ok(),
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
impl Notifier for SlackNotifier {
    async fn send(&self, ev: &NotificationEvent) -> Result<()> {
        let Some(url) = &self.webhook_url else {
            tracing::debug!("Slack disabled (no SLACK_WEBHOOK_URL)");
            return Ok(());
        };

        let text = format!(
            "*DJI alert:* *{:?}* ({:.2})\nReason: {}\n@ {}",
            ev.decision,
            ev.confidence,
            ev.reasons.first().cloned().unwrap_or_default(),
            ev.ts.to_rfc3339()
        );
        let body = serde_json::json!({ "text": text });

        self.client
            .post(url)
            .json(&body)
            .send()
            .await
            .context("slack post")?
            .error_for_status()
            .context("slack non-2xx")?;
        Ok(())
    }
}
