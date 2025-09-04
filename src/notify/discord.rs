use super::AlertPayload;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone)]
pub struct DiscordNotifier {
    webhook: String,
    client: Client,
    timeout: Duration,
    max_retries: u8,
}

impl DiscordNotifier {
    pub fn new(webhook: String) -> Self {
        Self {
            webhook,
            client: Client::new(),
            timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    pub fn with_retries(mut self, retries: u8) -> Self {
        self.max_retries = retries;
        self
    }

    pub async fn send_alert(&self, alert: &AlertPayload) -> Result<()> {
        let title = format!("Decision: {}", alert.decision);

        // FIX: build reasons string as owned value
        let reasons_str: String = if alert.reasons.is_empty() {
            "—".to_string()
        } else {
            alert.reasons.join(" · ")
        };

        let description = format!(
            "**Confidence:** {:.0}%\n**Reasons:** {}\n**Time (UTC):** {}",
            alert.confidence * 100.0,
            reasons_str,
            alert.timestamp_iso
        );

        let payload = DiscordWebhookPayload::embed(&title, &description);

        let mut attempt: u8 = 0;
        loop {
            attempt += 1;
            let res = self
                .client
                .post(&self.webhook)
                .timeout(self.timeout)
                .json(&payload)
                .send()
                .await;

            match res {
                Ok(rsp) => {
                    if let Err(e) = rsp.error_for_status_ref() {
                        if attempt < self.max_retries {
                            tokio::time::sleep(
                                Duration::from_millis(500u64 << (attempt - 1)),
                            )
                            .await;
                            continue;
                        }
                        return Err(anyhow!("Discord webhook HTTP error: {e}"));
                    }
                    return Ok(());
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        tokio::time::sleep(
                            Duration::from_millis(500u64 << (attempt - 1)),
                        )
                        .await;
                        continue;
                    }
                    return Err(anyhow!("Discord webhook request failed: {e}"));
                }
            }
        }
    }
}

#[derive(Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
}

#[derive(Serialize)]
struct DiscordWebhookPayload {
    content: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

impl DiscordWebhookPayload {
    fn embed(title: &str, description: &str) -> Self {
        Self {
            content: None,
            embeds: vec![DiscordEmbed {
                title: title.to_string(),
                description: description.to_string(),
            }],
        }
    }
}
