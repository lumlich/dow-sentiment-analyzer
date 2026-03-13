// src/last.rs
// Persisted JSON snapshot store used by /api/decide cold-start fallback.
// Supports Shuttle Shared Postgres (OpenDAL) and local file-backed storage.

use serde::{de::DeserializeOwned, Serialize};
use shuttle_shared_db::SerdeJsonOperator;
use std::path::{Path, PathBuf};
use tokio::fs;

pub const LAST_KEY: &str = "last_decision.json";

#[derive(Clone)]
pub struct LastStore {
    inner: LastStoreInner,
}

#[derive(Clone)]
enum LastStoreInner {
    Shuttle(SerdeJsonOperator),
    File(PathBuf),
}

impl LastStore {
    /// Create Shuttle-backed store from injected SerdeJsonOperator.
    pub fn new(inner: SerdeJsonOperator) -> Self {
        Self {
            inner: LastStoreInner::Shuttle(inner),
        }
    }

    /// Create file-backed store for local/demo runs.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            inner: LastStoreInner::File(path.into()),
        }
    }

    /// Save snapshot (overwrite); best-effort, errors are bubbled up.
    pub async fn save<T: Serialize>(&self, value: &T) -> anyhow::Result<()> {
        match &self.inner {
            LastStoreInner::Shuttle(inner) => {
                inner.write_serialized(LAST_KEY, value).await?;
            }
            LastStoreInner::File(path) => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent).await?;
                    }
                }
                let bytes = serde_json::to_vec_pretty(value)?;
                fs::write(path, bytes).await?;
            }
        }
        Ok(())
    }

    /// Load snapshot; returns Ok(None) if the key doesn't exist or any read error occurs.
    /// We intentionally degrade to None to avoid failing the whole request path.
    pub async fn load<T: DeserializeOwned>(&self) -> anyhow::Result<Option<T>> {
        match &self.inner {
            LastStoreInner::Shuttle(inner) => match inner.read_serialized::<T>(LAST_KEY).await {
                Ok(v) => Ok(Some(v)),
                Err(_e) => Ok(None),
            },
            LastStoreInner::File(path) => load_file(path).await,
        }
    }
}

async fn load_file<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    match fs::read(path).await {
        Ok(bytes) => match serde_json::from_slice::<T>(&bytes) {
            Ok(v) => Ok(Some(v)),
            Err(_e) => Ok(None),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_e) => Ok(None),
    }
}
