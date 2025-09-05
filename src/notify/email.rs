use anyhow::{Context, Result};
use lettre::message::{header, Mailbox, Message};
use lettre::transport::smtp::{authentication::Credentials, AsyncSmtpTransport};
use lettre::{AsyncTransport, Tokio1Executor};

use super::NotificationEvent;

pub struct EmailSender {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    to: Mailbox,
}

impl EmailSender {
    pub fn from_env() -> Self {
        let host = std::env::var("SMTP_HOST").expect("SMTP_HOST missing");
        let user = std::env::var("SMTP_USER").expect("SMTP_USER missing");
        let pass = std::env::var("SMTP_PASS").expect("SMTP_PASS missing");
        let from_addr =
            std::env::var("NOTIFY_EMAIL_FROM").expect("NOTIFY_EMAIL_FROM missing");
        let to_addr =
            std::env::var("NOTIFY_EMAIL_TO").expect("NOTIFY_EMAIL_TO missing");

        let creds = Credentials::new(user, pass);
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .expect("invalid SMTP_HOST")
            .credentials(creds)
            .build();

        let from = from_addr.parse().expect("invalid NOTIFY_EMAIL_FROM");
        let to = to_addr.parse().expect("invalid NOTIFY_EMAIL_TO");

        Self { mailer, from, to }
    }

    pub async fn send_event(&self, ev: &NotificationEvent) -> Result<()> {
        let subject = format!("DJI alert: {:?} ({:.2})", ev.decision, ev.confidence);
        let body = format!(
            "Decision: {:?}\nConfidence: {:.2}\nTop reason: {}\nTimestamp: {}\n",
            ev.decision,
            ev.confidence,
            ev.reasons.get(0).cloned().unwrap_or_default(),
            ev.ts.to_rfc3339()
        );

        let msg = Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
            .subject(subject)
            .header(header::ContentType::TEXT_PLAIN)
            .body(body)
            .context("build email")?;

        self.mailer.send(msg).await.context("send email")?;
        Ok(())
    }
}
