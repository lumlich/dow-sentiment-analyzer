// tests/ingest_config.rs
use dow_sentiment_analyzer::ingest::config::{load_whitelist_default, load_whitelist_from};
use std::{env, fs};

#[test]
fn parse_toml_and_json_paths() {
    let dir = tempfile::tempdir().unwrap();

    let p_toml = dir.path().join("ingest_whitelist.toml");
    fs::write(
        &p_toml,
        r#"
sources = [" Fed ", "", "Reuters", "Reuters"]
"#,
    )
    .unwrap();
    let v = load_whitelist_from(&p_toml).unwrap();
    assert_eq!(v, vec!["Fed".to_string(), "Reuters".to_string()]);

    let p_json = dir.path().join("ingest_whitelist.json");
    fs::write(&p_json, r#"["Bloomberg"," Reuters  ", ""]"#).unwrap();
    let vj = load_whitelist_from(&p_json).unwrap();
    assert_eq!(vj, vec!["Bloomberg".to_string(), "Reuters".to_string()]);
}

#[serial_test::serial]
#[test]
fn default_uses_env_then_fallbacks() {
    // Izoluj CWD, ať test nečte reálný repo config/
    let old = env::current_dir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    env::remove_var("INGEST_WHITELIST_PATH");

    // 1) Bez ničeho → prázdné
    let v = load_whitelist_default().unwrap();
    assert!(v.is_empty());

    // 2) Fallback TOML v ./config/
    let cfg_dir = tmp.path().join("config");
    fs::create_dir_all(&cfg_dir).unwrap();
    let p_toml = cfg_dir.join("ingest_whitelist.toml");
    fs::write(&p_toml, r#"sources = ["Fed","Reuters"]"#).unwrap();
    let vt = load_whitelist_default().unwrap();
    assert_eq!(vt, vec!["Fed".to_string(), "Reuters".to_string()]);

    // 3) ENV má přednost (ukáže jiný obsah)
    let p_env = tmp.path().join("ingest_whitelist.json");
    fs::write(&p_env, r#"["X"]"#).unwrap();
    env::set_var("INGEST_WHITELIST_PATH", p_env.display().to_string());
    let ve = load_whitelist_default().unwrap();
    assert_eq!(ve, vec!["X".to_string()]);
    env::remove_var("INGEST_WHITELIST_PATH");

    // Zpět do původního CWD
    env::set_current_dir(&old).unwrap();
}
