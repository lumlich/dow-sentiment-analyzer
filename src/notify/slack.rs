use super::AlertPayload;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone)]
pub struct SlackNotifier {
    webhook: String,
    client: Client,
    timeout: Duration,
    max_retries: u8,
}

impl SlackNotifier {
    /// Create a new SlackNotifier with default timeout (5s) and retries (3)
    pub fn new(webhook: String) -> Self {
        Self {
            webhook,
            client: Client::new(),
            timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }

    /// Builder: set request timeout in seconds
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Builder: set max retry attempts
    pub fn with_retries(mut self, retries: u8) -> Self {
        self.max_retries = retries;
        self
    }

    /// Send an alert payload to Slack Incoming Webhook
    pub async fn send_alert(&self, payload: &AlertPayload) -> Result<()> {
        let body = SlackMessage::from(payload);

        for attempt in 1..=self.max_retries {
            let resp = self
                .client
                .post(&self.webhook)
                .json(&body)
                .timeout(self.timeout)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => return Ok(()),
                Ok(r) => {
                    if attempt == self.max_retries {
                        return Err(anyhow!("Slack webhook failed with status: {}", r.status()));
                    }
                    // backoff could be added here if needed
                }
                Err(e) => {
                    if attempt == self.max_retries {
                        return Err(anyhow!("Slack webhook error: {:?}", e));
                    }
                }
            }
        }

        Err(anyhow!("Slack webhook retries exceeded"))
    }
}

#[derive(Serialize)]
struct SlackMessage {
    text: String,
}

impl From<&AlertPayload> for SlackMessage {
    fn from(p: &AlertPayload) -> Self {
        // Join reasons into a single line for Slack
        let reasons = if p.reasons.is_empty() {
            "-".to_string()
        } else {
            p.reasons.join(", ")
        };

        // Added emoji and nicer branding
        let text = format!(
            ":chart_with_upwards_trend: *Dow Sentiment Analyzer Alert*\n• *Decision:* {}\n• *Confidence:* {:.2}\n• *Reasons:* {}\n• *Time:* {}",
            p.decision,
            p.confidence,
            reasons,
            p.timestamp_iso
        );

        Self { text }
    }
}
