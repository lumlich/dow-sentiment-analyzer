// src/ingest/config.rs
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const ENV_PATH: &str = "INGEST_WHITELIST_PATH";

/// Load whitelist from an explicit path. Supports TOML or JSON formats.
pub fn load_whitelist_from(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading whitelist from {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    parse_whitelist(&content, ext.as_str())
}

/// Load whitelist using env var + fallbacks:
/// 1) $INGEST_WHITELIST_PATH
/// 2) config/ingest_whitelist.toml
/// 3) config/ingest_whitelist.json
pub fn load_whitelist_default() -> Result<Vec<String>> {
    if let Ok(p) = std::env::var(ENV_PATH) {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return load_whitelist_from(&pb);
        } else {
            return Err(anyhow!("INGEST_WHITELIST_PATH points to non-existent path"));
        }
    }
    let toml_p = PathBuf::from("config/ingest_whitelist.toml");
    if toml_p.exists() {
        return load_whitelist_from(&toml_p);
    }
    let json_p = PathBuf::from("config/ingest_whitelist.json");
    if json_p.exists() {
        return load_whitelist_from(&json_p);
    }
    Ok(Vec::new())
}

fn parse_whitelist(s: &str, hint_ext: &str) -> Result<Vec<String>> {
    // Try TOML first if hinted or content looks like toml.
    let try_toml = hint_ext == "toml" || s.contains("sources");
    if try_toml {
        if let Ok(v) = parse_toml(s) {
            return Ok(v);
        }
    }
    // Try JSON array
    if let Ok(v) = parse_json(s) {
        return Ok(v);
    }
    // Fallback: also try TOML if not attempted
    if !try_toml {
        if let Ok(v) = parse_toml(s) {
            return Ok(v);
        }
    }
    Err(anyhow!("unsupported whitelist format"))
}

fn parse_toml(s: &str) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct TomlWl {
        sources: Vec<String>,
    }
    let v: TomlWl = toml::from_str(s)?;
    Ok(clean_list(v.sources))
}

fn parse_json(s: &str) -> Result<Vec<String>> {
    let v: Vec<String> = serde_json::from_str(s)?;
    Ok(clean_list(v))
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut set = BTreeSet::new();
    for it in items {
        let t = it.trim();
        if !t.is_empty() {
            set.insert(t.to_string());
        }
    }
    set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    #[test]
    fn dedup_trim_and_formats_work() {
        let toml = r#"sources = [" Fed ", "", "Reuters", "Reuters"]"#;
        let json = r#"["Bloomberg", "  Reuters  ", ""]"#;
        let toml_out = parse_toml(toml).unwrap();
        assert_eq!(toml_out, vec!["Fed".to_string(), "Reuters".to_string()]);
        let json_out = parse_json(json).unwrap();
        assert_eq!(
            json_out,
            vec!["Bloomberg".to_string(), "Reuters".to_string()]
        );
    }

    #[serial_test::serial]
    #[test]
    fn default_uses_env_then_fallbacks() {
        // Izoluj CWD do temp složky, aby nerušil reálný config/ v repo
        let old = env::current_dir().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        env::set_current_dir(tmp.path()).unwrap();

        env::remove_var(ENV_PATH);

        // Bez souborů v temp CWD → prázdné
        let v = load_whitelist_default().unwrap();
        assert!(v.is_empty());

        // Env má přednost
        let p_json = tmp.path().join("ingest_whitelist.json");
        fs::write(&p_json, r#"["X"]"#).unwrap();
        env::set_var(ENV_PATH, p_json.display().to_string());
        let v2 = load_whitelist_default().unwrap();
        assert_eq!(v2, vec!["X".to_string()]);
        env::remove_var(ENV_PATH);

        // Obnov CWD
        env::set_current_dir(&old).unwrap();
    }
}
