use super::*;

#[test]
fn fallback_regex_suggestion_normalizes_whitespace() {
    assert_eq!(
        fallback_regex_suggestion("  coffee   shop  purchase "),
        "(?i)^coffee\\s+shop\\s+purchase$"
    );
}
