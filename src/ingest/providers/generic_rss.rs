use async_trait::async_trait;
use tracing::{info, warn};

use crate::ingest::types::{SourceEvent, SourceProvider};

#[derive(Debug, serde::Deserialize)]
struct FeedDef {
    id: String,
    name: String,
    url: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    weight: Option<f32>,
    #[serde(default)]
    enabled: bool,
}

pub struct GenericRssProvider {
    cfg_path: std::path::PathBuf,
    client: reqwest::Client,
}

impl GenericRssProvider {
    pub fn new() -> Self {
        let path =
            std::env::var("FEEDS_CONFIG_PATH").unwrap_or_else(|_| "config/feeds.json".to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .user_agent("dow-sentiment-analyzer/1.0 (+generic_rss)")
            .build()
            .expect("build reqwest client");
        Self {
            cfg_path: path.into(),
            client,
        }
    }

    fn load_cfg(&self) -> Vec<FeedDef> {
        match std::fs::read(&self.cfg_path) {
            Ok(bytes) => match serde_json::from_slice::<Vec<FeedDef>>(&bytes) {
                Ok(mut v) => {
                    v.retain(|f| f.enabled);
                    v
                }
                Err(e) => {
                    warn!(error=?e, "generic_rss: failed to parse feeds.json");
                    Vec::new()
                }
            },
            Err(e) => {
                warn!(error=?e, path=?self.cfg_path, "generic_rss: feeds.json not found/readable");
                Vec::new()
            }
        }
    }

    fn now_unix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn source_label(feed: &FeedDef, url_opt: &Option<String>) -> String {
        let guess = url_opt
            .as_deref()
            .and_then(|u| u.split('/').nth(2)) // velmi jednoduché získání hostu
            .unwrap_or_default()
            .to_ascii_lowercase();

        // Heuristické „hezké“ názvy
        if guess.contains("bls.gov") {
            return "BLS".into();
        }
        if guess.contains("bea.gov") {
            return "BEA".into();
        }
        if guess.contains("treasury") {
            return "Treasury".into();
        }
        if guess.contains("whitehouse.gov") {
            return "WhiteHouse".into();
        }
        if guess.contains("ecb.europa.eu") {
            return "ECB".into();
        }
        if guess.contains("cnbc.com") {
            return "CNBC".into();
        }
        if guess.contains("dowjones") || guess.contains("marketwatch") {
            return "MarketWatch".into();
        }
        if guess.contains("trump") || guess.contains("truth") {
            return "Trump".into();
        }

        // fallback: id -> jinak name
        if !feed.id.is_empty() {
            feed.id.clone()
        } else {
            feed.name.clone()
        }
    }
}

#[async_trait]
impl SourceProvider for GenericRssProvider {
    fn name(&self) -> &'static str {
        "generic_rss"
    }

    async fn fetch(&self) -> anyhow::Result<Vec<SourceEvent>> {
        let feeds = self.load_cfg();
        if feeds.is_empty() {
            info!("generic_rss: no enabled feeds");
            return Ok(Vec::new());
        }

        let mut out: Vec<SourceEvent> = Vec::new();
        let client = &self.client;

        for f in feeds {
            match client.get(&f.url).send().await {
                Ok(resp) => match resp.bytes().await {
                    Ok(bytes) => {
                        let mut rdr = std::io::Cursor::new(bytes);
                        match feed_rs::parser::parse(&mut rdr) {
                            Ok(feed) => {
                                for entry in feed.entries {
                                    let title = entry
                                        .title
                                        .as_ref()
                                        .map(|t| t.content.clone())
                                        .unwrap_or_default();
                                    let summary = entry
                                        .summary
                                        .as_ref()
                                        .map(|s| s.content.clone())
                                        .unwrap_or_default();
                                    let text = if summary.is_empty() {
                                        title.clone()
                                    } else {
                                        format!("{title} — {summary}")
                                    };
                                    let ts = entry
                                        .published
                                        .or(entry.updated)
                                        .map(|d| d.timestamp() as u64)
                                        .unwrap_or_else(Self::now_unix);
                                    let url = entry.links.first().map(|l| l.href.clone());
                                    let source = Self::source_label(&f, &url);

                                    out.push(SourceEvent {
                                        source,
                                        published_at: ts,
                                        text,
                                        url,
                                    });
                                }
                            }
                            Err(e) => warn!(error=?e, feed=?f.id, "generic_rss: parse failed"),
                        }
                    }
                    Err(e) => warn!(error=?e, feed=?f.id, "generic_rss: read body failed"),
                },
                Err(e) => warn!(error=?e, feed=?f.id, url=%f.url, "generic_rss: request failed"),
            }
        }

        Ok(out)
    }
}
