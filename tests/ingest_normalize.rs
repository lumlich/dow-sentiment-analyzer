// tests/ingest_normalize.rs
use dow_sentiment_analyzer::ingest::normalize_text;

#[test]
fn empty_is_ok() {
    assert_eq!(normalize_text(""), "");
}

#[test]
fn strips_html_and_unescapes() {
    let s = "<p>Hello&nbsp;<b>world</b> &ldquo;ok&rdquo;</p>";
    let n = normalize_text(s);
    assert_eq!(n, r#"Hello world "ok""#);
}

#[test]
fn folds_whitespace_and_nbsp() {
    let s = "A\u{00A0}\n\tB   C";
    let n = normalize_text(s);
    assert_eq!(n, "A B C");
}

#[test]
fn length_cap_applies() {
    let s = "x".repeat(2_000);
    let n = normalize_text(&s);
    assert!(n.len() <= 1_500);
}
