use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use feed_rs::model::{Entry, Link};
use feed_rs::parser;
use reqwest::Client;

use super::{Provider, ProviderItem}; // Provider = SourceProvider, ProviderItem = SourceEvent

/// Federal Reserve – unified RSS/Atom provider.
pub struct FedRssProvider {
    client: Client,
    feed_url: String,
    /// If present, provider will parse from this XML instead of doing HTTP.
    fixture_xml: Option<String>,
}

impl FedRssProvider {
    pub fn new() -> Self {
        Self::new_with_url("https://www.federalreserve.gov/feeds/press_all.xml")
    }

    pub fn new_with_url(url: &str) -> Self {
        let client = Client::builder()
            .user_agent("dow-sentiment-analyzer/1.0 (+https://shuttle.app)")
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            client,
            feed_url: url.to_string(),
            fixture_xml: None,
        }
    }

    /// Constructor for tests/fixtures with a 'static XML slice.
    pub fn from_fixture(xml: &'static str) -> Self {
        let client = Client::builder()
            .user_agent("dow-sentiment-analyzer/1.0 (+https://shuttle.app)")
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            client,
            feed_url: "fixture://fed".to_string(),
            fixture_xml: Some(xml.to_string()),
        }
    }

    /// Constructor for tests/fixtures with a borrowed XML string.
    pub fn from_fixture_str(xml: &str) -> Self {
        let client = Client::builder()
            .user_agent("dow-sentiment-analyzer/1.0 (+https://shuttle.app)")
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            client,
            feed_url: "fixture://fed".to_string(),
            fixture_xml: Some(xml.to_string()),
        }
    }

    async fn fetch_bytes(&self) -> Result<Vec<u8>> {
        if let Some(xml) = &self.fixture_xml {
            return Ok(xml.as_bytes().to_vec());
        }

        let res = self
            .client
            .get(&self.feed_url)
            .send()
            .await
            .with_context(|| format!("GET {}", self.feed_url))?;

        let status = res.status();
        let bytes = res.bytes().await.context("reading feed body")?;
        anyhow::ensure!(
            status.is_success(),
            "HTTP {} for {}",
            status.as_u16(),
            self.feed_url
        );
        Ok(bytes.to_vec())
    }

    fn is_html(l: &Link) -> bool {
        l.media_type
            .as_deref()
            .map(|mt| mt.starts_with("text/html"))
            .unwrap_or(false)
    }

    fn pick_url(entry: &Entry) -> Option<String> {
        entry
            .links
            .iter()
            .find(|l| l.rel.as_deref() == Some("alternate"))
            .or_else(|| entry.links.iter().find(|l| Self::is_html(l)))
            .or_else(|| entry.links.first())
            .map(|l| l.href.clone())
    }

    fn published_utc(entry: &Entry) -> DateTime<Utc> {
        if let Some(p) = entry.published {
            return p.with_timezone(&Utc);
        }
        if let Some(u) = entry.updated {
            return u.with_timezone(&Utc);
        }
        Utc::now()
    }

    fn entry_to_item(entry: Entry) -> ProviderItem {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        let summary = entry.summary.as_ref().map(|s| s.content.clone());
        let url = Self::pick_url(&entry);
        let ts = Self::published_utc(&entry).timestamp().max(0) as u64;

        // Compose one-line text (title + short summary)
        let mut text = String::new();
        if !title.is_empty() {
            text.push_str(&title);
        }
        if let Some(s) = summary.as_ref() {
            let s = s.replace('\n', " ").trim().to_string();
            if !s.is_empty() {
                if !text.is_empty() {
                    text.push_str(" — ");
                }
                let snippet = if s.len() > 280 {
                    format!("{}…", &s[..280])
                } else {
                    s
                };
                text.push_str(&snippet);
            }
        }

        ProviderItem {
            source: "Fed".to_string(),
            published_at: ts,
            text,
            url,
            priority_hint: None,
        }
    }
}

impl Default for FedRssProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Provider for FedRssProvider {
    fn name(&self) -> &'static str {
        "Fed"
    }

    async fn fetch_latest(&self) -> Result<Vec<ProviderItem>> {
        let bytes = self.fetch_bytes().await?;
        let feed = parser::parse(&bytes[..]).context("parsing feed")?;
        let mut out = Vec::with_capacity(feed.entries.len());
        for e in feed.entries {
            out.push(Self::entry_to_item(e));
        }
        Ok(out)
    }
}
