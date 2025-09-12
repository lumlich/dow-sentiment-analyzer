// tests/providers_fed.rs
use dow_sentiment_analyzer::ingest::normalize_text;
use dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider;
use dow_sentiment_analyzer::ingest::types::SourceProvider;
use std::fs;

fn decode_html_minimal(s: &str) -> String {
    // Minimal, dependency-free HTML entity decoding for common cases in fixtures
    let mut out = s.replace("&nbsp;", " ").replace("&#160;", " ");
    for (entity, repl) in [
        ("&ldquo;", "\""),
        ("&rdquo;", "\""),
        ("&lsquo;", "'"),
        ("&rsquo;", "'"),
        ("&ndash;", "-"),
        ("&mdash;", "-"),
        ("&hellip;", "..."),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
    ] {
        out = out.replace(entity, repl);
    }
    out
}

#[tokio::test]
async fn parses_fed_fixture() {
    let xml_raw = fs::read_to_string("tests/fixtures/fed_rss.xml").expect("fixture");
    let xml = decode_html_minimal(&xml_raw);

    let p = FedRssProvider::from_fixture(&xml);
    let evs = p.fetch_latest().await.expect("ok");

    assert_eq!(evs.len(), 2);
    assert!(evs.iter().all(|e| e.source == "Fed"));
    assert!(evs.iter().all(|e| e.published_at > 0));
    assert!(evs.iter().all(|e| e.url.is_some()));

    let t0 = normalize_text(&evs[0].text);
    assert!(t0.contains("Statement on interest rates"));
    assert!(t0.contains("maintain interest rates"));
}
