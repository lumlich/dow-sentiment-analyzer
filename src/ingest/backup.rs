// src/ingest/backup.rs
use anyhow::Result;

#[async_trait::async_trait]
pub trait BackupSink: Send + Sync {
    /// Store (path, content) pairs atomically (as best-effort).
    async fn store(&self, items: Vec<(String, String)>) -> Result<()>;
}

/// Reads files from `config/*.json` and passes them to the sink.
pub async fn backup_configs_once<S: BackupSink>(sink: &S) -> Result<()> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir("config") {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    items.push((path.to_string_lossy().to_string(), content));
                }
            }
        }
    }
    sink.store(items).await
}

/// Simple daily tokio task. Wire this from your app startup.
pub fn spawn_daily_backup_task<S: BackupSink + 'static>(sink: S) {
    // 24h interval
    let period = std::time::Duration::from_secs(24 * 3600);
    tokio::spawn(async move {
        loop {
            let _ = backup_configs_once(&sink).await;
            tokio::time::sleep(period).await;
        }
    });
}

// --- Test helper ---
pub struct MockSink {
    pub calls: std::sync::Mutex<Vec<Vec<(String, String)>>>,
}

impl MockSink {
    pub fn new() -> Self {
        Self {
            calls: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl BackupSink for MockSink {
    async fn store(&self, items: Vec<(String, String)>) -> Result<()> {
        self.calls.lock().unwrap().push(items);
        Ok(())
    }
}
