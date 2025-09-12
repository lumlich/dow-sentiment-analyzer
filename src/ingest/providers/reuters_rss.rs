use anyhow::{Context, Result};
use async_trait::async_trait;
use metrics::{counter, histogram};
use quick_xml::de::from_str;
use serde::Deserialize;
use time::{format_description::well_known::Rfc2822, OffsetDateTime, UtcOffset};

use crate::ingest::types::{SourceEvent, SourceProvider};

#[derive(Debug, Deserialize)]
struct Rss {
    channel: Channel,
}
#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(rename = "item")]
    item: Vec<Item>,
}
#[derive(Debug, Deserialize)]
struct Item {
    title: Option<String>,
    link: Option<String>,
    #[serde(rename = "pubDate")]
    pub_date: Option<String>,
    description: Option<String>,
}

fn parse_rfc2822_to_unix(ts: &str) -> u64 {
    OffsetDateTime::parse(ts, &Rfc2822)
        .ok()
        .map(|dt| dt.to_offset(UtcOffset::UTC).unix_timestamp())
        .and_then(|x| u64::try_from(x).ok())
        .unwrap_or(0)
}

pub struct ReutersRssProvider {
    mode: Mode,
}

enum Mode {
    #[cfg(feature = "ingest-fixtures")]
    Fixture(String),
    #[cfg(feature = "ingest-http")]
    Http {
        url: &'static str,
        client: reqwest::Client,
    },
}

impl ReutersRssProvider {
    #[cfg(feature = "ingest-fixtures")]
    pub fn from_fixture(s: &'static str) -> Self {
        Self {
            mode: Mode::Fixture(s.to_string()),
        }
    }

    #[cfg(feature = "ingest-fixtures")]
    pub fn from_fixture_str(s: &str) -> Self {
        Self {
            mode: Mode::Fixture(s.to_string()),
        }
    }

    #[cfg(feature = "ingest-http")]
    pub fn from_url(url: &'static str) -> Self {
        let client = reqwest::Client::new();
        Self {
            mode: Mode::Http { url, client },
        }
    }

    fn parse_items_from_str(s: &str) -> Result<Vec<SourceEvent>> {
        let t0 = std::time::Instant::now();
        let rss: Rss = from_str(s).context("parsing reuters rss xml")?;

        let mut out = Vec::with_capacity(rss.channel.item.len());
        for it in rss.channel.item {
            let text_raw = format!(
                "{}. {}",
                it.title.as_deref().unwrap_or_default(),
                it.description.as_deref().unwrap_or_default()
            );
            let text = crate::ingest::normalize_text(&text_raw);
            if text.is_empty() {
                continue;
            }

            out.push(SourceEvent {
                source: "Reuters".to_string(),
                published_at: it
                    .pub_date
                    .as_deref()
                    .map(parse_rfc2822_to_unix)
                    .unwrap_or(0),
                text,
                url: it.link,
                priority_hint: Some(5.0),
            });
        }

        let ms = t0.elapsed().as_secs_f64() * 1_000.0;
        histogram!("ingest_parse_ms").record(ms);
        counter!("ingest_events_total").increment(out.len() as u64);
        Ok(out)
    }
}

#[async_trait]
impl SourceProvider for ReutersRssProvider {
    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>> {
        match &self.mode {
            #[cfg(feature = "ingest-fixtures")]
            Mode::Fixture(s) => Self::parse_items_from_str(s),

            #[cfg(feature = "ingest-http")]
            Mode::Http { url, client } => {
                let body = match client.get(*url).send().await {
                    Ok(resp) => resp.text().await.context("reuters http .text()")?,
                    Err(e) => {
                        tracing::warn!(error = ?e, provider = "Reuters", "provider http error");
                        counter!("ingest_provider_errors_total").increment(1);
                        return Err(e).context("reuters http get()");
                    }
                };
                Self::parse_items_from_str(&body)
            }
        }
    }

    fn name(&self) -> &'static str {
        "Reuters"
    }
}
