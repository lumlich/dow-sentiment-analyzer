//! Synthetic relevance suite (≈100–130 programmatically built sentences).
//! Run with: cargo test -q -- --ignored
//! Env toggles:
//!   SHOW_REASONS=1   -> print first reason per row
//!   SHOW_ALL=1       -> print full reasons vector (verbose)

use dow_sentiment_analyzer::relevance::RelevanceEngine;
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use std::fmt::Write as _;

#[derive(Clone)]
struct Case {
    text: String,
    expect_pass: bool,
    why: &'static str,
}

/* ----------------------------
Inline, deterministic TOML cfg
---------------------------- */
const TEST_TOML: &str = r#"
[relevance]
threshold = 0.25
near_default_window = 6

# Keep weights minimal (only categories we actually use),
# so normalization matches hand-picked math: (hard=3, macro=2) -> denom = 15.
[weights]
hard = 3
macro = 2

# ---- Anchors ----

# Core DJIA / Dow anchors (hard)
[[anchors]]
id = "djia_core_names"
category = "hard"
pattern = "(?i)\\b(djia|dow jones|the dow|dow)\\b"

# Tag-based 'hard' signal: #DJIA, $DJI, $YM
[[anchors]]
id = "tags_djia_dji_ym"
category = "hard"
pattern = "(?i)(#djia|\\$dji|\\$ym)\\b"

# Macro context: common macro signals (FOMC, Fed, rates, yields, inflation, CPI, payrolls, unemployment, GDP)
[[anchors]]
id = "macro_core"
category = "macro"
pattern = "(?i)\\b(fomc|fed|rates?|yields?|inflation|cpi|payrolls?|unemployment|gdp)\\b"

# Optional Powell-near-Fed/rates (also counts as macro, precision-first)
[[anchors]]
id = "powell_near_fed_rates"
category = "macro"
pattern = "(?i)\\bpowell\\b"
near = { pattern = "(?i)\\b(fed|fomc|rates?)\\b", window = 6 }

# Single-stock guard for $DOW (soft, requires broader context)
[[anchors]]
id = "$dow_single"
category = "soft"
pattern = "(?i)\\$dow\\b"
tag = "single_stock_only"

# Single-stock guard for 'Dow Inc.' (soft)
[[anchors]]
id = "dow_inc_single"
category = "soft"
pattern = "(?i)\\bdow inc\\.?\\b"
tag = "single_stock_only"

# ---- Blockers ----

# Block DJI drones near drone terms
[[blockers]]
id = "dji_drones"
pattern = "(?i)\\bdji\\b"
near = { pattern = "(?i)\\b(drone|mavic)\\b", window = 4 }
reason = "DJI (drones)"
action = "block"

# Block 'dow' when near 'inc.' => the company, not the index
[[blockers]]
id = "dow_inc_near_dow_word"
pattern = "(?i)\\bdow\\b"
near = { pattern = "(?i)\\binc\\.?\\b", window = 1 }
reason = "Dow Inc (single stock)"
action = "block"

# ---- Combos ----
[combos]
pass_any = [
  { need = ["macro","hard"] }
]

# Aliases (kept for future)
[aliases]
verb_or_semi = ["verb","semi"]
"#;

/* ----------------------------
Case builder
---------------------------- */

/// Build ~100–130 mixed cases: clearly relevant, blocked, irrelevant, and noisy mixes.
/// Uses seeded RNG for deterministic runs.
fn build_cases() -> Vec<Case> {
    // Thematic pools
    let powell_ctx = &[
        "powell",
        "fomc",
        "fed",
        "interest rates",
        "rate hike",
        "rate cuts",
        "yields",
    ];
    // limit to what TEST_TOML actually recognizes as 'hard'
    let dow_ctx = &["dow", "dow jones", "the dow", "djia"];
    let macro_news = &[
        "inflation cools",
        "inflation spikes",
        "cpi beats",
        "cpi misses",
        "payrolls jump",
        "payrolls miss",
        "unemployment rises",
        "gdp slows",
    ];

    // Irrelevant/false-positive traps
    let drones = &["dji", "mavic", "drone", "gimbal"];
    let random_topics = &[
        "weather is sunny",
        "football match",
        "celebrity drama",
        "local festival",
        "new smartphone release",
    ];

    // 1) Deterministic “good” combos (macro + hard) => should pass
    let mut good = Vec::new();
    for &p in powell_ctx {
        for &d in dow_ctx {
            good.push(Case {
                text: format!("{} talks; {} reacts after FOMC presser.", cap(p), d),
                expect_pass: true,
                why: "macro+hard combo",
            });
        }
    }
    for &m in macro_news {
        for &d in dow_ctx {
            good.push(Case {
                text: format!("{}; {} tumbles in late trade.", cap(m), d),
                expect_pass: true,
                why: "macro+hard combo",
            });
        }
    }

    // 2) Proximity‑based relevant (djia + fed/rates nearby) => pass
    let mut proximity_ok = Vec::new();
    for &d in &["dow", "djia"] {
        for &m in &["fomc", "fed", "rates"] {
            proximity_ok.push(Case {
                text: format!("{} drifts as {} outlook shifts.", cap(d), m),
                expect_pass: true,
                why: "djia + macro proximity",
            });
        }
    }

    // 3) Hashtags/cashtags relevant (hard symbols) => pass
    let tags_pass = vec![
        Case {
            text: "Watching #DJIA into the closing bell; futures look heavy.".into(),
            expect_pass: true,
            why: "hashtag DJIA",
        },
        Case {
            text: "Traders eye $DJI and YM futures after CPI beats.".into(),
            expect_pass: true,
            why: "cashtag $DJI + futures",
        },
        Case {
            text: "Micro e-mini $YM reacts to Fed guidance.".into(),
            expect_pass: true,
            why: "$YM + fed context",
        },
    ];

    // 4) Blockers (DJI drones) => should fail
    let mut blocked = Vec::new();
    for &a in drones {
        for &b in drones {
            if a != b {
                blocked.push(Case {
                    text: format!("DJI launches new {} with improved {}.", a, b),
                    expect_pass: false,
                    why: "DJI drones blocker",
                });
            }
        }
    }

    // 5) Homonyms / traps => should fail (no need to add explicit blockers; they just won't pass)
    let mut traps = vec![
        Case {
            text: "Robert Downey returns to the screen.".into(),
            expect_pass: false,
            why: "robert_downey (irrelevant)",
        },
        Case {
            text: "We binge-watched Downton Abbey all weekend.".into(),
            expect_pass: false,
            why: "downton_abbey (irrelevant)",
        },
        Case {
            text: "The old window casts a long shadow.".into(),
            expect_pass: false,
            why: "dow substring homonyms",
        },
        Case {
            text: "Down syndrome research advances.".into(),
            expect_pass: false,
            why: "medical context (irrelevant)",
        },
    ];

    // 6) $DOW (single-stock) without/with context
    let mut dow_inc = vec![
        Case {
            text: "$DOW announces a cash dividend.".into(),
            expect_pass: false,
            why: "$DOW without broader context",
        },
        Case {
            text: "Wall Street shrugs as $DOW rallies; the index remains mixed.".into(),
            expect_pass: true,
            why: "$DOW with market context",
        },
        Case {
            text: "$DOW spikes but DJIA is flat after the opening bell.".into(),
            expect_pass: true,
            why: "$DOW near DJIA/opening",
        },
    ];

    // 7) Random irrelevants => should fail
    let mut irrelevants = random_topics
        .iter()
        .map(|s| Case {
            text: cap(s),
            expect_pass: false,
            why: "irrelevant",
        })
        .collect::<Vec<_>>();

    // 8) Small randomized batch around macro/dow with noise (seeded for determinism)
    let mut rng = StdRng::seed_from_u64(42);
    let mut rnd = Vec::new();
    for _ in 0..40 {
        let d = dow_ctx.choose(&mut rng).unwrap();
        let m = macro_news.choose(&mut rng).unwrap();
        let noise = random_topics.choose(&mut rng).unwrap();
        let flip: bool = rng.gen_bool(0.75); // 75% by design should be relevant
        let text = if flip {
            format!("{} after {} ; {}", cap(d), m, noise)
        } else {
            format!("{} ; {}", cap(noise), m)
        };
        rnd.push(Case {
            text,
            expect_pass: flip,
            why: if flip {
                "macro+hard (noisy)"
            } else {
                "mostly noise"
            },
        });
    }

    let mut out = Vec::new();
    out.extend(good);
    out.extend(proximity_ok);
    out.extend(tags_pass);
    out.extend(blocked);
    out.append(&mut traps);
    out.append(&mut dow_inc);
    out.append(&mut irrelevants);
    out.extend(rnd);

    // Limit / pad to ~100–150 cases deterministically
    out.truncate(130);
    out
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[test]
#[ignore] // run manually: cargo test --test relevance_synthetic -- --ignored --nocapture
fn synthetic_relevance_suite() {
    // Use the inline config for determinism
    let eng = RelevanceEngine::from_toml_str(TEST_TOML).expect("load inline");

    let cases = build_cases();

    let mut ok = 0usize;
    let mut fail = 0usize;

    let mut tp = 0usize; // expect_pass && passed
    let mut tn = 0usize; // !expect_pass && !passed
    let mut fp = 0usize; // !expect_pass && passed
    let mut fn_ = 0usize; // expect_pass && !passed

    let show_reasons = std::env::var("SHOW_REASONS").ok().as_deref() == Some("1");
    let show_all = std::env::var("SHOW_ALL").ok().as_deref() == Some("1");

    let mut buf = String::new();
    writeln!(
        &mut buf,
        "{:<4} | {:<5} | {:<5} | {:<5} | {:<7} | {}",
        "Idx", "Expect", "Got", "Score", "Reason", "Text"
    )
    .unwrap();
    writeln!(&mut buf, "{}", "-".repeat(120)).unwrap();

    for (i, c) in cases.iter().enumerate() {
        let r = eng.score(&c.text);
        let passed = r.score > 0.0;
        let got = if passed { "pass" } else { "fail" };
        let expect = if c.expect_pass { "pass" } else { "fail" };
        let score_str = format!("{:.2}", r.score);

        if passed == c.expect_pass {
            ok += 1;
        } else {
            fail += 1;
        }

        match (c.expect_pass, passed) {
            (true, true) => tp += 1,
            (false, false) => tn += 1,
            (false, true) => fp += 1,
            (true, false) => fn_ += 1,
        }

        let first_reason = r.reasons.get(0).map(|s| s.as_str()).unwrap_or("-");
        let reason_cell = if show_all {
            format!("{:?}", r.reasons)
        } else if show_reasons {
            first_reason.to_string()
        } else {
            "-".into()
        };

        writeln!(
            &mut buf,
            "{:<4} | {:<5} | {:<5} | {:<5} | {:<7} | {}  ({})",
            i, expect, got, score_str, reason_cell, c.text, c.why
        )
        .unwrap();
    }

    let total = cases.len();
    let accuracy = ok as f32 / total as f32;

    let precision = if tp + fp > 0 {
        tp as f32 / (tp + fp) as f32
    } else {
        0.0
    };
    let recall = if tp + fn_ > 0 {
        tp as f32 / (tp + fn_) as f32
    } else {
        0.0
    };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    println!(
        "\n{}\nTotal: {}  OK: {}  FAIL: {}\nTP: {}  TN: {}  FP: {}  FN: {}\n\
         Accuracy: {:.1}%  Precision: {:.1}%  Recall: {:.1}%  F1: {:.1}%\n",
        buf,
        total,
        ok,
        fail,
        tp,
        tn,
        fp,
        fn_,
        100.0 * accuracy,
        100.0 * precision,
        100.0 * recall,
        100.0 * f1
    );

    // Strict criterion: want at least 85% match (tweak as needed)
    assert!(
        accuracy >= 0.85,
        "Synthetic suite accuracy {:.1}% below threshold (85%)",
        100.0 * accuracy
    );
}
