// src/last.rs
// Minimal JSON snapshot store on Shuttle Shared Postgres via OpenDAL.
// No SQL migrations required.

use serde::{de::DeserializeOwned, Serialize};
use shuttle_shared_db::SerdeJsonOperator;

pub const LAST_KEY: &str = "last_decision.json";

#[derive(Clone)]
pub struct LastStore {
    inner: SerdeJsonOperator,
}

impl LastStore {
    /// Create new store from injected SerdeJsonOperator.
    pub fn new(inner: SerdeJsonOperator) -> Self {
        Self { inner }
    }

    /// Save snapshot (overwrite); best-effort, errors are bubbled up.
    pub async fn save<T: Serialize>(&self, value: &T) -> anyhow::Result<()> {
        self.inner.write_serialized(LAST_KEY, value).await?;
        Ok(())
    }

    /// Load snapshot; returns Ok(None) if the key doesn't exist or any read error occurs.
    /// We intentionally degrade to None to avoid failing the whole request path.
    pub async fn load<T: DeserializeOwned>(&self) -> anyhow::Result<Option<T>> {
        match self.inner.read_serialized::<T>(LAST_KEY).await {
            Ok(v) => Ok(Some(v)),
            Err(_e) => Ok(None),
        }
    }
}
