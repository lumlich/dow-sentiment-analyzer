// src/ingest/providers/fed_rss.rs
use anyhow::Result;
use quick_xml::de::from_str;
use serde::Deserialize;

use crate::ingest::types::{SourceEvent, SourceProvider};

#[derive(Debug, Deserialize)]
struct Rss {
    channel: Channel,
}

#[derive(Debug, Deserialize)]
struct Channel {
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
    // Example: "Mon, 01 Sep 2025 12:34:56 GMT"
    time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc2822)
        .ok()
        .and_then(|dt| {
            dt.to_offset(time::UtcOffset::UTC)
                .unix_timestamp()
                .try_into()
                .ok()
        })
        .unwrap_or(0)
}

pub struct FedRssProvider {
    /// In tests we pass fixture content directly.
    pub rss_content: String,
}

impl FedRssProvider {
    pub fn from_fixture(content: &str) -> Self {
        Self {
            rss_content: content.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl SourceProvider for FedRssProvider {
    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>> {
        let rss: Rss = from_str(&self.rss_content)?;
        let mut out = Vec::with_capacity(rss.channel.item.len());
        for it in rss.channel.item {
            let text = it.title.clone().unwrap_or_default()
                + " "
                + &it.description.clone().unwrap_or_default();
            let ev = SourceEvent {
                source: "Fed".to_string(),
                published_at: it
                    .pub_date
                    .as_deref()
                    .map(parse_rfc2822_to_unix)
                    .unwrap_or(0),
                text,
                url: it.link,
                priority_hint: Some(10), // central bank default hint
            };
            out.push(ev);
        }
        Ok(out)
    }

    fn name(&self) -> &'static str {
        "Fed"
    }
}
