//! AI adapter: provider abstraction + file cache + daily limit.
//! All comments are in English. No new crates are required beyond reqwest/serde that already exist.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------
// Public surface
// ------------------------------------------------------------

/// Result returned by AI providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiResult {
    pub short_reason: String,
}

/// Trait object used elsewhere in the app (handlers/tests).
pub trait AiClient: Send + Sync {
    /// Analyze input and (optionally) return a short reason (<=160 ASCII chars).
    fn analyze<'a>(
        &'a self,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>>;
    /// Provider name for diagnostics/headers.
    fn provider_name(&self) -> &'static str;
}

/// Build-time config loaded from `config/ai.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    /// "openai" | "claude" (claude is stubbed for now)
    pub provider: Option<String>,
    /// Optional per-day limit; defaults to 20 if absent.
    pub daily_limit: Option<u32>,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            daily_limit: Some(20),
        }
    }
}

/// Load config from `config/ai.json`. If reading/parsing fails, returns `AiConfig::default()`.
pub fn load_ai_config() -> AiConfig {
    let path = Path::new("config/ai.json");
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AiConfig::default(),
    }
}

/// Convenient alias used by callers.
pub type DynAiClient = Arc<dyn AiClient>;

/// Backwards-compat helpers for existing code paths:
pub type SharedAi = DynAiClient;
pub use DisabledClient as AiClientDisabled;

/// Old helper used by api/bootstrap: reads config from disk and builds a client.
pub fn build_ai_client() -> DynAiClient {
    let cfg = load_ai_config();
    build_client_from_config(&cfg)
}

/// Factory: build a client according to config and environment variables.
///
/// * If `AI_TEST_MODE=mock`, returns a deterministic mock client.
/// * Else if `config.enabled==false`, returns a disabled client.
/// * Else builds the real provider (OpenAI) wrapped with caching + daily limit.
pub fn build_client_from_config(config: &AiConfig) -> DynAiClient {
    if std::env::var("AI_TEST_MODE")
        .map(|v| v == "mock")
        .unwrap_or(false)
    {
        let mock = MockProvider {
            fixed: AiResult {
                short_reason: "Neutral hint (mock)".to_string(),
            },
        };
        let client =
            CachingClient::new(mock, default_cache_dir(), config.daily_limit.unwrap_or(20));
        return Arc::new(client);
    }

    if !config.enabled {
        return Arc::new(DisabledClient);
    }

    match config.provider.as_deref() {
        Some("openai") => {
            let provider = OpenAiProvider::new(None);
            let client = CachingClient::new(
                provider,
                default_cache_dir(),
                config.daily_limit.unwrap_or(20),
            );
            Arc::new(client)
        }
        Some("claude") => {
            // Stub: return disabled until implemented.
            Arc::new(DisabledClient)
        }
        _ => Arc::new(DisabledClient),
    }
}

// ------------------------------------------------------------
// Provider abstraction + concrete providers
// ------------------------------------------------------------

/// Low-level provider: does a *real* remote call. Separated so we can reuse the same
/// caching wrapper for production and tests.
pub trait Provider: Send + Sync + 'static {
    fn fetch<'a>(
        &'a self,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>>;
    fn name(&self) -> &'static str;
}

/// OpenAI provider (uses Chat Completions API). Requires `OPENAI_API_KEY`.
pub struct OpenAiProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    /// `model_override`: pass Some("gpt-4o-mini") to override; defaults to gpt-4o-mini.
    pub fn new(model_override: Option<&str>) -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let http = reqwest::Client::builder()
            .user_agent("dow-sentiment-analyzer/0.1 (+github.com/lumlich/dow-sentiment-analyzer)")
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        let model = model_override.unwrap_or("gpt-4o-mini").to_string();
        Self {
            http,
            api_key,
            model,
        }
    }
}

impl Provider for OpenAiProvider {
    fn fetch<'a>(
        &'a self,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>> {
        Box::pin(async move {
            if self.api_key.is_empty() {
                return None;
            }

            #[derive(Serialize)]
            struct Msg<'a> {
                role: &'a str,
                content: &'a str,
            }
            #[derive(Serialize)]
            struct Req<'a> {
                model: &'a str,
                messages: Vec<Msg<'a>>,
                temperature: f32,
                max_tokens: u32,
            }
            #[derive(Deserialize)]
            struct Resp {
                choices: Vec<Choice>,
            }
            #[derive(Deserialize)]
            struct Choice {
                message: ChoiceMsg,
            }
            #[derive(Deserialize)]
            struct ChoiceMsg {
                content: String,
            }

            let sys = "You are a market hint generator. Return ONE short sentence (<=160 ASCII chars), neutral tone, no emojis. Output only the sentence.";
            let req = Req {
                model: &self.model,
                messages: vec![
                    Msg {
                        role: "system",
                        content: sys,
                    },
                    Msg {
                        role: "user",
                        content: input,
                    },
                ],
                temperature: 0.2,
                max_tokens: 80,
            };

            let resp = self
                .http
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&self.api_key)
                .json(&req)
                .send()
                .await
                .ok()?;

            if !resp.status().is_success() {
                return None;
            }
            let body: Resp = resp.json().await.ok()?;
            let content = body
                .choices
                .first()
                .map(|c| c.message.content.as_str())
                .unwrap_or("");
            let cleaned = sanitize_reason(content);
            if cleaned.is_empty() {
                None
            } else {
                Some(AiResult {
                    short_reason: cleaned,
                })
            }
        })
    }
    fn name(&self) -> &'static str {
        "openai"
    }
}

/// Returns `None` always; used when AI is disabled.
pub struct DisabledClient;

impl AiClient for DisabledClient {
    fn analyze<'a>(
        &'a self,
        _input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>> {
        Box::pin(async { None })
    }
    fn provider_name(&self) -> &'static str {
        "disabled"
    }
}

/// Simple mock provider for tests/local runs.
#[derive(Clone)]
pub struct MockProvider {
    pub fixed: AiResult,
}

impl Provider for MockProvider {
    fn fetch<'a>(
        &'a self,
        _input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>> {
        let out = self.fixed.clone();
        Box::pin(async move { Some(out) })
    }
    fn name(&self) -> &'static str {
        "mock"
    }
}

// ------------------------------------------------------------
// Caching client wrapper (file cache + daily limit)
// ------------------------------------------------------------

/// File names and counter state are guarded by a `Mutex` to keep it simple and safe.
pub struct CachingClient<P: Provider> {
    inner: P,
    cache_dir: PathBuf,
    daily_limit_max: u32,
    counter: Arc<Mutex<DailyCounter>>, // shared across clones if needed
}

impl<P: Provider> CachingClient<P> {
    pub fn new(inner: P, cache_dir: PathBuf, daily_limit_max: u32) -> Self {
        let _ = fs::create_dir_all(&cache_dir); // best-effort
        let counter = Arc::new(Mutex::new(
            load_daily_counter(&cache_dir).unwrap_or_default(),
        ));
        Self {
            inner,
            cache_dir,
            daily_limit_max,
            counter,
        }
    }

    async fn analyze_impl(&self, input: &str) -> Option<AiResult> {
        // 1) Check daily limit (real API calls only increment; cache hits do not).
        {
            let mut g = self.counter.lock().expect("poisoned counter");
            if g.is_expired() {
                g.reset_to_today();
                let _ = save_daily_counter(&self.cache_dir, &g);
            }
            if g.count >= self.daily_limit_max {
                return None;
            }
        }

        // 2) Cache lookup.
        let key = cache_key(input);
        if let Some(hit) = read_cache_file(&self.cache_dir, &key) {
            return Some(hit);
        }

        // 3) Real call.
        if let Some(mut fresh) = self.inner.fetch(input).await {
            fresh.short_reason = sanitize_reason(&fresh.short_reason);
            if !fresh.short_reason.is_empty() {
                let _ = write_cache_file(&self.cache_dir, &key, &fresh);
                // Increment after a successful real call.
                let mut g = self.counter.lock().expect("poisoned counter");
                g.count = g.count.saturating_add(1);
                let _ = save_daily_counter(&self.cache_dir, &g);
                return Some(fresh);
            }
        }
        None
    }
}

impl<P: Provider> AiClient for CachingClient<P> {
    fn analyze<'a>(
        &'a self,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>> {
        Box::pin(self.analyze_impl(input))
    }
    fn provider_name(&self) -> &'static str {
        self.inner.name()
    }
}

// ------------------------------------------------------------
// File cache helpers
// ------------------------------------------------------------

fn default_cache_dir() -> PathBuf {
    PathBuf::from("cache/ai")
}

fn cache_key(input: &str) -> String {
    // NOTE: We intentionally avoid adding a new dependency (sha2). DefaultHasher is sufficient for cache keys.
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn cache_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.json"))
}

fn read_cache_file(dir: &Path, key: &str) -> Option<AiResult> {
    let path = cache_path(dir, key);
    let mut file = fs::File::open(path).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

fn write_cache_file(dir: &Path, key: &str, value: &AiResult) -> io::Result<()> {
    let path = cache_path(dir, key);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    let mut f = fs::File::create(&tmp)?;
    f.write_all(json.as_bytes())?;
    fs::rename(tmp, path)?;
    Ok(())
}

// ------------------------------------------------------------
// Daily counter helpers
// ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DailyCounter {
    date: String,
    count: u32,
}
impl Default for DailyCounter {
    fn default() -> Self {
        Self {
            date: today(),
            count: 0,
        }
    }
}
impl DailyCounter {
    fn is_expired(&self) -> bool {
        self.date != today()
    }
    fn reset_to_today(&mut self) {
        self.date = today();
        self.count = 0;
    }
}

fn today() -> String {
    // Days since UNIX epoch (string). Sufficient for equality and rollover; avoids extra crates.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();
    let days = secs / 86_400;
    days.to_string()
}

fn counter_path(dir: &Path) -> PathBuf {
    dir.join("daily_count.json")
}

fn load_daily_counter(dir: &Path) -> io::Result<DailyCounter> {
    let p = counter_path(dir);
    let s = fs::read_to_string(p)?;
    let dc: DailyCounter =
        serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(dc)
}

fn save_daily_counter(dir: &Path, dc: &DailyCounter) -> io::Result<()> {
    let p = counter_path(dir);
    let tmp = p.with_extension("json.tmp");
    let s = serde_json::to_string(dc).unwrap_or_else(|_| "{}".to_string());
    let mut f = fs::File::create(&tmp)?;
    f.write_all(s.as_bytes())?;
    fs::rename(tmp, p)?;
    Ok(())
}

// ------------------------------------------------------------
// Sanitization
// ------------------------------------------------------------

/// Ensure ASCII-only, single line, and <=160 chars. Collapses whitespace.
pub fn sanitize_reason(input: &str) -> String {
    let mut out = String::with_capacity(160);
    let mut prev_space = false;
    for ch in input.chars() {
        let c = match ch {
            '\r' | '\n' | '\t' => ' ',
            c if c.is_ascii() => c,
            _ => ' ',
        };
        if c == ' ' {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
        if out.len() >= 160 {
            break;
        }
    }
    out.trim().to_string()
}
