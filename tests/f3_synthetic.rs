// tests/f3_synthetic.rs
// Synthetic integration tests for Phase 3 (NER, Rerank, Antispam, Calibration, Rules).
// Tyto testy se vyhýbají skutečnému ./config tak, že pro NER používají vlastní temp adresář
// a nesahají na process-wide current working directory (kromě řízeného fallbacku).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use dow_sentiment_analyzer::analyze::{
    antispam::{AntiSpam, AntiSpamParams},
    ner::{enrich_reasons, extract_reasons_from_configs},
    rerank::{
        rerank_keep_last_and_decay_duplicates, Statement, DEFAULT_DUPLICATE_DECAY,
        DEFAULT_RELEVANCE_THRESHOLD, DEFAULT_SIMILARITY_THRESHOLD,
    },
    rules::{apply_rules_to_text, HotReloadRules},
    scoring::{base_confidence, ScoreInputs},
    weights::HotReloadWeights,
};
use serial_test::serial;

// --- test helpers ---

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir();
    let unique = format!(
        "f3_tests_{}",
        std::time::UNIX_EPOCH.elapsed().unwrap().as_millis()
    );
    base.join(unique)
}

fn write_file(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.sync_all().unwrap();
}

// --- NER ---

#[test]
#[serial] // test pracuje s env proměnnou/cwd; držíme ho sériově kvůli stabilitě v CI
fn f3_ner_extracts_from_temp_configs() {
    // 1) Primárně zkusíme přes NER_CONFIG_DIR (bez změny CWD)
    let tmp = tmp_dir();
    let config_dir = tmp.join("config");
    fs::create_dir_all(&config_dir).expect("mkdir tmp/config");

    let config_dir_abs = config_dir.canonicalize().unwrap_or(config_dir.clone());
    std::env::set_var(
        "NER_CONFIG_DIR",
        config_dir_abs.to_string_lossy().to_string(),
    );

    write_file(
        config_dir_abs.join("inflation.json"),
        r#"{"patterns":[{"regex":"(?i)\\binflation\\b","keyword":"inflation"}]}"#,
    );
    write_file(
        config_dir_abs.join("rates.json"),
        r#"{"patterns":[{"regex":"(?i)\\brates?\\b","keyword":"rates"}]}"#,
    );

    let text = "Inflation is rising and central bank raises rates.";
    let mut reasons = extract_reasons_from_configs(text);
    eprintln!("NER reasons (ENV path): {:?}", reasons);

    let mut has_inflation = reasons.iter().any(|r| {
        r.eq_ignore_ascii_case("inflation: inflation") || r.to_lowercase().contains("inflation")
    });
    let mut has_rates = reasons
        .iter()
        .any(|r| r.eq_ignore_ascii_case("rates: rates") || r.to_lowercase().contains("rates"));

    // 2) Pokud impl. nečte NER_CONFIG_DIR, uděláme řízený fallback: dočasně přepneme CWD
    if !(has_inflation && has_rates) {
        let old_cwd = std::env::current_dir().expect("get cwd");
        let fallback_root = tmp.join("cwd_fallback");
        let fallback_cfg = fallback_root.join("config");
        fs::create_dir_all(&fallback_cfg).expect("mkdir fallback/config");

        write_file(
            fallback_cfg.join("inflation.json"),
            r#"{"patterns":[{"regex":"(?i)\\binflation\\b","keyword":"inflation"}]}"#,
        );
        write_file(
            fallback_cfg.join("rates.json"),
            r#"{"patterns":[{"regex":"(?i)\\brates?\\b","keyword":"rates"}]}"#,
        );

        // zajistíme, aby se nepoužil ENV fallback
        std::env::remove_var("NER_CONFIG_DIR");
        std::env::set_current_dir(&fallback_root).expect("chdir fallback_root");

        reasons = extract_reasons_from_configs(text);
        eprintln!("NER reasons (CWD fallback): {:?}", reasons);

        // obnovit CWD
        let _ = std::env::set_current_dir(old_cwd);

        has_inflation = reasons.iter().any(|r| {
            r.eq_ignore_ascii_case("inflation: inflation") || r.to_lowercase().contains("inflation")
        });
        has_rates = reasons
            .iter()
            .any(|r| r.eq_ignore_ascii_case("rates: rates") || r.to_lowercase().contains("rates"));
    }

    assert!(
        has_inflation,
        "Expected an inflation reason, got: {:?}",
        reasons
    );
    assert!(has_rates, "Expected a rates reason, got: {:?}", reasons);

    // Enrichment should keep existing reasons
    let out = enrich_reasons(vec!["pipeline: base".into()], text);
    assert!(out.iter().any(|r| r == "pipeline: base"));

    // úklid
    std::env::remove_var("NER_CONFIG_DIR");
    let _ = fs::remove_dir_all(tmp);
}

// --- RERANK ---

#[test]
fn f3_rerank_prioritizes_latest_and_decays_duplicates() {
    let items = vec![
        Statement {
            source: "Fed".into(),
            timestamp: 1000,
            text: "We will hike now".into(),
            weight: 1.0,
            relevance: 0.6,
        },
        Statement {
            source: "Fed".into(),
            timestamp: 2000,
            text: "We will hike now".into(),
            weight: 1.0,
            relevance: 0.65,
        },
        Statement {
            source: "Fed".into(),
            timestamp: 3000,
            text: "We will hike now".into(),
            weight: 1.0,
            relevance: 0.8,
        },
    ];

    let out = rerank_keep_last_and_decay_duplicates(
        items,
        DEFAULT_RELEVANCE_THRESHOLD,
        DEFAULT_SIMILARITY_THRESHOLD,
        DEFAULT_DUPLICATE_DECAY,
    );

    assert_eq!(out.first().unwrap().timestamp, 3000);
    let older = out.iter().find(|s| s.timestamp == 2000).unwrap();
    assert!(older.weight < 1.0, "expected older duplicate to be decayed");
}

// --- ANTISPAM ---

fn ts(base: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + base)
}

#[test]
fn f3_antispam_filters_near_duplicates_within_window() {
    let params = AntiSpamParams {
        window_size: 16,
        similarity_threshold: 0.90,
        time_window_secs: 600,
    };
    let mut anti = AntiSpam::new(params);

    let inputs = vec![
        (ts(0), "BREAKING: Fed will hike".to_string()),
        (ts(60), "BREAKING: Fed will hike".to_string()),
        (ts(120), "BREAKING: Fed will hike".to_string()),
        (ts(700), "Fed keeps rates unchanged".to_string()),
    ];

    let kept = anti.filter_batch(inputs.clone());

    assert_eq!(
        kept.len(),
        2,
        "expected only the first and the distant different one to pass"
    );
    assert_eq!(kept[0].1, "BREAKING: Fed will hike");
    assert_eq!(kept[1].1, "Fed keeps rates unchanged");
}

// --- CALIBRATION (weights) ---

#[test]
fn f3_calibration_changes_base_confidence_via_weights() {
    let tmp = tmp_dir();
    let path = tmp.join("weights.json");
    write_file(
        &path,
        r#"{"w_source":1.0,"w_strength":1.0,"w_recency":1.0}"#,
    );

    let hot = HotReloadWeights::new(Some(&path));
    let w1 = hot.current();

    let inputs = ScoreInputs {
        source_score: 0.9,
        strength_score: 0.5,
        recency_score: 0.1,
    };
    let c1 = base_confidence(&inputs, &w1);

    thread::sleep(Duration::from_millis(1100));

    write_file(
        &path,
        r#"{"w_source":2.0,"w_strength":1.0,"w_recency":0.0}"#,
    );
    let w2 = hot.current();
    let c2 = base_confidence(&inputs, &w2);

    assert_ne!(c1, c2);

    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(tmp);
}

// --- RULES ---

#[test]
fn f3_rules_set_action_boost_conf_and_add_reason() {
    let tmp = tmp_dir();
    let rules_path = tmp.join("rules.json");

    write_file(
        &rules_path,
        r#"
{
  "rules": [
    {
      "when": { "any_contains": ["buyback", "beats earnings"] },
      "then": {
        "set_action": "BUY",
        "boost_confidence": 0.2,
        "add_reason": "rule: earnings positive"
      }
    }
  ]
}
"#,
    );

    let hot = HotReloadRules::new(Some(&rules_path));
    let rules = hot.current();

    let text = "Company beats earnings; considering buyback this quarter.";
    let (maybe_action, delta, extra) = apply_rules_to_text(text, &rules);

    assert_eq!(maybe_action.as_deref(), Some("BUY"));
    assert!(delta > 0.0);
    assert!(extra.iter().any(|r| r.contains("earnings")));

    let _ = fs::remove_file(&rules_path);
    let _ = fs::remove_dir_all(tmp);
}
