pub mod antiflutter;
pub mod discord;
pub mod email;
pub mod slack; // module exists at src/notify/antiflutter.rs

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// High-level decision kinds used across notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionKind {
    BUY,
    SELL,
    HOLD,
    #[cfg(test)] // keep TEST only for test builds to avoid dead_code warnings in CI
    TEST,
}

/// Payload we send to notifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub decision: DecisionKind,
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub ts: DateTime<Utc>,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, ev: &NotificationEvent) -> Result<()>;
}

/// Slack webhook notifier.
pub struct SlackNotifier {
    webhook_url: Option<String>,
    client: reqwest::Client,
}
impl SlackNotifier {
    pub fn from_env() -> Self {
        Self {
            webhook_url: std::env::var("SLACK_WEBHOOK_URL").ok(),
            client: reqwest::Client::new(),
        }
    }
}
#[async_trait]
impl Notifier for SlackNotifier {
    async fn send(&self, ev: &NotificationEvent) -> Result<()> {
        let Some(url) = &self.webhook_url else {
            tracing::debug!("Slack disabled (no SLACK_WEBHOOK_URL)");
            return Ok(());
        };
        let reason = ev.reasons.first().cloned().unwrap_or_default();
        let text = format!(
            "*DJI alert:* *{:?}* ({:.2})\nReason: {}\n@ {}",
            ev.decision,
            ev.confidence,
            reason,
            ev.ts.to_rfc3339()
        );
        let body = serde_json::json!({ "text": text });
        self.client
            .post(url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Discord webhook notifier.
pub struct DiscordNotifier {
    webhook_url: Option<String>,
    client: reqwest::Client,
}
impl DiscordNotifier {
    pub fn from_env() -> Self {
        Self {
            webhook_url: std::env::var("DISCORD_WEBHOOK_URL").ok(),
            client: reqwest::Client::new(),
        }
    }
}
#[async_trait]
impl Notifier for DiscordNotifier {
    async fn send(&self, ev: &NotificationEvent) -> Result<()> {
        let Some(url) = &self.webhook_url else {
            tracing::debug!("Discord disabled (no DISCORD_WEBHOOK_URL)");
            return Ok(());
        };
        let reason = ev.reasons.first().cloned().unwrap_or_default();
        let content = format!(
            "**DJI alert:** **{:?}** ({:.2})\nReason: {}\n{}",
            ev.decision,
            ev.confidence,
            reason,
            ev.ts.to_rfc3339()
        );
        let body = serde_json::json!({ "content": content });
        self.client
            .post(url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Email notifier (wrapper around `email::EmailSender`), gated via EMAIL_ENABLED.
pub struct EmailNotifier {
    inner: Option<email::EmailSender>,
}
impl EmailNotifier {
    pub fn from_env() -> Self {
        let enabled = std::env::var("EMAIL_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            inner: if enabled {
                Some(email::EmailSender::from_env())
            } else {
                None
            },
        }
    }
}
#[async_trait]
impl Notifier for EmailNotifier {
    async fn send(&self, ev: &NotificationEvent) -> Result<()> {
        if let Some(inner) = &self.inner {
            inner.send_event(ev).await?;
        } else {
            tracing::debug!("Email disabled (EMAIL_ENABLED not true)");
        }
        Ok(())
    }
}

/// Fan-out multiplexer that sends to all enabled channels.
pub struct NotifierMux {
    notifiers: Vec<Box<dyn Notifier>>,
}
impl NotifierMux {
    pub fn from_env() -> Self {
        let v: Vec<Box<dyn Notifier>> = vec![
            Box::new(SlackNotifier::from_env()),
            Box::new(DiscordNotifier::from_env()),
            Box::new(EmailNotifier::from_env()),
        ];
        Self { notifiers: v }
    }
    pub async fn notify(&self, ev: &NotificationEvent) {
        for n in &self.notifiers {
            if let Err(e) = n.send(ev).await {
                tracing::warn!("notify failed: {e:#}");
            }
        }
    }
}
