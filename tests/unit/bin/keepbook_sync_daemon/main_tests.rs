use super::*;

#[test]
fn parse_nonzero_duration_rejects_zero() {
    assert!(parse_nonzero_duration_arg("0s").is_err());
    assert_eq!(
        parse_nonzero_duration_arg("30s").expect("duration should parse"),
        Duration::from_secs(30)
    );
}
