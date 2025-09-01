//! Runtime-calibrated weights with hot-reload from config/weights.json.
//!
//! JSON shape:
//! {
//!   "w_source": 1.0,
//!   "w_strength": 1.0,
//!   "w_recency": 1.0
//! }
//!
//! On each `current()` call we check the file's modified time and reload if changed.

use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::RwLock,
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Weights {
    pub w_source: f32,
    pub w_strength: f32,
    pub w_recency: f32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            w_source: 1.0,
            w_strength: 1.0,
            w_recency: 1.0,
        }
    }
}

/// Hot-reload wrapper: reloads when the config file mtime changes.
#[derive(Debug)]
pub struct HotReloadWeights {
    path: PathBuf,
    inner: RwLock<State>,
}

#[derive(Debug)]
struct State {
    weights: Weights,
    last_modified: Option<SystemTime>,
}

impl HotReloadWeights {
    /// Create with a path (defaults to "config/weights.json" if `None`).
    pub fn new(path: Option<&Path>) -> Self {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("config/weights.json"));
        Self {
            path,
            inner: RwLock::new(State {
                weights: Weights::default(),
                last_modified: None,
            }),
        }
    }

    /// Get the latest weights, reloading if the config file changed.
    pub fn current(&self) -> Weights {
        // Fast path: check metadata without grabbing write lock yet.
        let (needs_reload, _new_mtime) = match fs::metadata(&self.path).and_then(|m| m.modified()) {
            Ok(mtime) => {
                // Read lock to compare with cached mtime.
                let guard = self.inner.read().unwrap();
                let changed = guard.last_modified != Some(mtime);
                (changed, Some(mtime))
            }
            Err(_) => {
                // If file isn't there, we keep defaults; no reload.
                (false, None)
            }
        };

        if !needs_reload {
            return self.inner.read().unwrap().weights;
        }

        // Slow path: reload with write lock.
        let mut guard = self.inner.write().unwrap();
        // Double-check in case of races.
        if let Ok(meta) = fs::metadata(&self.path) {
            if let Ok(mtime) = meta.modified() {
                if guard.last_modified != Some(mtime) {
                    if let Ok(w) = load_weights_file(&self.path) {
                        guard.weights = w;
                        guard.last_modified = Some(mtime);
                    }
                }
            }
        }
        guard.weights
    }
}

/// Load weights directly (no caching). Public for tests/tools.
pub fn load_weights_file(path: &Path) -> io::Result<Weights> {
    let bytes = fs::read(path)?;
    let w: Weights = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::{io::Write, thread, time::Duration};

    /// Create a unique temporary directory in std::env::temp_dir().
    fn unique_tmp_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("weights_test_{}", nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_and_hot_reloads() {
        let tmpdir = unique_tmp_dir();
        let path = tmpdir.join("weights.json");

        // Write initial
        {
            let mut f = fs::File::create(&path).unwrap();
            write!(f, r#"{{"w_source":1.2,"w_strength":0.8,"w_recency":1.5}}"#).unwrap();
            f.sync_all().unwrap();
        }

        let hot = HotReloadWeights::new(Some(&path));
        let w1 = hot.current();
        assert!((w1.w_source - 1.2).abs() < f32::EPSILON);
        assert!((w1.w_strength - 0.8).abs() < f32::EPSILON);
        assert!((w1.w_recency - 1.5).abs() < f32::EPSILON);

        // Ensure different mtime (Windows granularity can be coarse).
        thread::sleep(Duration::from_millis(1100));

        // Update file
        {
            let mut f = fs::File::create(&path).unwrap();
            write!(f, r#"{{"w_source":2.0,"w_strength":2.0,"w_recency":2.0}}"#).unwrap();
            f.sync_all().unwrap();
        }

        let w2 = hot.current();
        assert!((w2.w_source - 2.0).abs() < f32::EPSILON);
        assert!((w2.w_strength - 2.0).abs() < f32::EPSILON);
        assert!((w2.w_recency - 2.0).abs() < f32::EPSILON);

        // Cleanup (best-effort)
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&tmpdir);
    }
}
