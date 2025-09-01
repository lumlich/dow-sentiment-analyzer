// tests/f3_synthetic.rs
// Synthetic integration tests for Phase 3 (NER, Rerank, Antispam, Calibration, Rules).
// These tests avoid touching the project's real ./config by using a temporary working directory.

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

// --- test helpers ---

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir();
    let unique = format!(
        "f3_tests_{}",
        std::time::UNIX_EPOCH.elapsed().unwrap().as_millis()
    );
    base.join(unique)
}

fn with_temp_workdir<F: FnOnce()>(f: F) {
    let old = std::env::current_dir().expect("get cwd");
    let tmp = tmp_dir();
    fs::create_dir_all(&tmp).expect("mkdir tmp");
    std::env::set_current_dir(&tmp).expect("chdir tmp");
    f();
    // best-effort cleanup
    let _ = std::env::set_current_dir(old);
    let _ = fs::remove_dir_all(tmp);
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
fn f3_ner_extracts_from_temp_configs() {
    with_temp_workdir(|| {
        // Prepare ./config/*.json in a throwaway cwd
        write_file(
            "config/inflation.json",
            r#"{"patterns":[{"regex":"(?i)\\binflation\\b","keyword":"inflation"}]}"#,
        );
        write_file(
            "config/rates.json",
            r#"{"patterns":[{"regex":"(?i)\\brates?\\b","keyword":"rates"}]}"#,
        );
        // geopolitics/earnings intentionally missing to test graceful skip

        // Should pick up both categories when present
        let text = "Inflation is rising and central bank raises rates.";
        let reasons = extract_reasons_from_configs(text);

        // Expect category: keyword shape
        assert!(reasons.iter().any(|r| r == "inflation: inflation"));
        assert!(reasons.iter().any(|r| r == "rates: rates"));

        // Enrichment should keep existing reasons
        let out = enrich_reasons(vec!["pipeline: base".into()], text);
        assert!(out.iter().any(|r| r == "pipeline: base"));
    });
}

// --- RERANK ---

#[test]
fn f3_rerank_prioritizes_latest_and_decays_duplicates() {
    // Make earlier statements EXACTLY identical to the latest to guarantee similarity >= 0.90
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
            text: "We will hike now".into(), // exact duplicate of latest text
            weight: 1.0,
            relevance: 0.65,
        },
        Statement {
            source: "Fed".into(),
            timestamp: 3000,
            text: "We will hike now".into(), // latest & relevant
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

    // Latest relevant statement should appear before earlier ones
    assert_eq!(out.first().unwrap().timestamp, 3000);

    // Earlier exact-duplicate should be decayed below original
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
        time_window_secs: 600, // 10 minutes
    };
    let mut anti = AntiSpam::new(params);

    // Make duplicates EXACTLY identical in the time window to guarantee filtering
    let inputs = vec![
        (ts(0), "BREAKING: Fed will hike".to_string()),
        (ts(60), "BREAKING: Fed will hike".to_string()), // identical, 1 min later
        (ts(120), "BREAKING: Fed will hike".to_string()), // identical, 2 min later
        (ts(700), "Fed keeps rates unchanged".to_string()), // different, 11+ min later
    ];

    let kept = anti.filter_batch(inputs.clone().into_iter());

    // Expect to keep first and the later different one; near-dupes in window are filtered
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
    // Create a temp weights.json and use HotReloadWeights::new(Some(path))
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

    // Ensure different mtime (Windows granularity)
    thread::sleep(Duration::from_millis(1100));

    write_file(
        &path,
        r#"{"w_source":2.0,"w_strength":1.0,"w_recency":0.0}"#,
    );
    let w2 = hot.current();
    let c2 = base_confidence(&inputs, &w2);

    // With higher weight on source and zero on recency, expect a change
    assert_ne!(c1, c2);

    // Cleanup
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(tmp);
}

// --- RULES ---

#[test]
fn f3_rules_set_action_boost_conf_and_add_reason() {
    with_temp_workdir(|| {
        // Prepare a rules.json and load via HotReloadRules(Some(path))
        let rules_path = PathBuf::from("rules.json");
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
    });
}
