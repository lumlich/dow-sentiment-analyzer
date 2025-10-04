use async_trait::async_trait;
use tracing::{info, warn};
use anyhow::Result;

use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT, HeaderMap, HeaderValue, HeaderName};

use crate::ingest::types::{SourceEvent, SourceProvider};

#[derive(Debug, serde::Deserialize)]
struct FeedDef {
    id: String,
    name: String,
    url: String,
    #[serde(default)]
    #[allow(dead_code)]
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
        // Cesta k feeds.json
        let path =
            std::env::var("FEEDS_CONFIG_PATH").unwrap_or_else(|_| "config/feeds.json".to_string());

        // Defaultní hlavičky „vypadám jako prohlížeč“ + preferuj RSS/Atom
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "application/atom+xml, application/rss+xml, application/xml;q=0.9, text/xml;q=0.8, */*;q=0.1",
            ),
        );
        headers.insert(HeaderName::from_static("accept-language"), HeaderValue::from_static("en-US,en;q=0.8"));
        headers.insert(HeaderName::from_static("cache-control"), HeaderValue::from_static("no-cache"));
        headers.insert(HeaderName::from_static("pragma"), HeaderValue::from_static("no-cache"));

        // Umožni přepsat UA přes env; jinak rozumný browser-like UA
        let ua = std::env::var("HTTP_USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0.0.0 Safari/537.36 \
             DowSentimentAnalyzer/1.0"
                .to_string()
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .default_headers(headers)
            .user_agent(ua)
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
            .and_then(|u| u.split('/').nth(2)) // naive host extraction
            .unwrap_or_default()
            .to_ascii_lowercase();

        // Heuristic "nice" labels
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

        // fallback: id -> otherwise name
        if !feed.id.is_empty() {
            feed.id.clone()
        } else {
            feed.name.clone()
        }
    }

    /// Heuristika: je odpověď „HTML stránka“ (anti‑bot / chybová stránka), nikoli XML feed?
    fn is_probably_html(content_type: &str, body: &[u8]) -> bool {
        let ct = content_type.to_ascii_lowercase();
        if ct.contains("text/html") {
            return true;
        }
        // bezpečný rychlý náhled prvních pár bajtů
        let head = &body[..body.len().min(64)];
        let head_lc = String::from_utf8_lossy(head).to_ascii_lowercase();
        head_lc.contains("<html") || head_lc.contains("<!doctype html")
    }

    /// Stáhni feed s rozumnými hlavičkami; pokud přijde HTML, zkus jeden fallback s jiným UA.
    async fn fetch_bytes_with_fallback(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        // 1) pokus s výchozím UA
        let resp = self.client.get(url).send().await?;
        let status = resp.status();
        let ct: String = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| "-".to_string());
        let body: Vec<u8> = resp.bytes().await?.to_vec();

        if !status.is_success() {
            let preview = String::from_utf8_lossy(&body[..body.len().min(200)]);
            warn!(%status, %ct, preview=%preview, %url, "generic_rss: non-success response");
            anyhow::bail!("status {}", status);
        }

        if Self::is_probably_html(&ct, &body) {
            // 2) fallback s jiným UA (některé servery blokují jen určité UA)
            let resp2 = self
                .client
                .get(url)
                .header(USER_AGENT, "curl/8.4.0")
                .send()
                .await?;
            let status2 = resp2.status();
            let ct2: String = resp2
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_owned())
                .unwrap_or_else(|| "-".to_string());
            let body2: Vec<u8> = resp2.bytes().await?.to_vec();

            if !status2.is_success() || Self::is_probably_html(&ct2, &body2) {
                let preview2 = String::from_utf8_lossy(&body2[..body2.len().min(200)]);
                warn!(%status2, %ct2, preview=%preview2, %url, "generic_rss: got HTML instead of XML");
                anyhow::bail!("non-XML response");
            }
            return Ok(body2);
        }

        Ok(body)
    }
}

#[async_trait]
impl SourceProvider for GenericRssProvider {
    fn name(&self) -> &'static str {
        "generic_rss"
    }

    async fn fetch_latest(&self) -> Result<Vec<SourceEvent>> {
        let feeds = self.load_cfg();
        if feeds.is_empty() {
            info!("generic_rss: no enabled feeds");
            return Ok(Vec::new());
        }

        let mut out: Vec<SourceEvent> = Vec::new();

        for f in feeds {
            match self.fetch_bytes_with_fallback(&f.url).await {
                Ok(bytes) => {
                    // feed-rs očekává reader
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
                                    priority_hint: f.weight,
                                });
                            }
                        }
                        Err(e) => warn!(error=?e, feed=?f.id, "generic_rss: parse failed"),
                    }
                }
                Err(e) => warn!(error=?e, feed=?f.id, url=%f.url, "generic_rss: request failed"),
            }
        }

        Ok(out)
    }
}
