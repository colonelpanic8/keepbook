use super::*;

#[test]
fn test_parse_days() {
    assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
    assert_eq!(
        parse_duration("14d").unwrap(),
        Duration::from_secs(14 * 86400)
    );
    assert_eq!(
        parse_duration("365d").unwrap(),
        Duration::from_secs(365 * 86400)
    );
}

#[test]
fn test_parse_hours() {
    assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    assert_eq!(
        parse_duration("24h").unwrap(),
        Duration::from_secs(24 * 3600)
    );
    assert_eq!(
        parse_duration("48h").unwrap(),
        Duration::from_secs(48 * 3600)
    );
}

#[test]
fn test_parse_minutes() {
    assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
    assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(30 * 60));
    assert_eq!(parse_duration("90m").unwrap(), Duration::from_secs(90 * 60));
}

#[test]
fn test_parse_seconds() {
    assert_eq!(parse_duration("1s").unwrap(), Duration::from_secs(1));
    assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
    assert_eq!(parse_duration("3600s").unwrap(), Duration::from_secs(3600));
}

#[test]
fn test_case_insensitive() {
    assert_eq!(parse_duration("1D").unwrap(), Duration::from_secs(86400));
    assert_eq!(parse_duration("1H").unwrap(), Duration::from_secs(3600));
    assert_eq!(parse_duration("1M").unwrap(), Duration::from_secs(60));
    assert_eq!(parse_duration("1S").unwrap(), Duration::from_secs(1));
}

#[test]
fn test_whitespace_handling() {
    assert_eq!(
        parse_duration("  1d  ").unwrap(),
        Duration::from_secs(86400)
    );
    assert_eq!(
        parse_duration("\t24h\n").unwrap(),
        Duration::from_secs(24 * 3600)
    );
    assert_eq!(
        parse_duration(" 30m ").unwrap(),
        Duration::from_secs(30 * 60)
    );
}

#[test]
fn test_invalid_unit() {
    assert!(parse_duration("1x").is_err());
    assert!(parse_duration("1w").is_err());
    assert!(parse_duration("1").is_err());
    assert!(parse_duration("d").is_err());
}

#[test]
fn test_invalid_number() {
    assert!(parse_duration("abcd").is_err());
    assert!(parse_duration("-1d").is_err());
    assert!(parse_duration("1.5h").is_err());
}

#[test]
fn test_overflow_rejected() {
    let max = u64::MAX.to_string();
    assert!(parse_duration(&format!("{max}d")).is_err());
    assert!(parse_duration(&format!("{max}h")).is_err());
    assert!(parse_duration(&format!("{max}m")).is_err());
    assert!(parse_duration(&format!("{max}s")).is_ok());
}

#[test]
fn test_empty_input() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("   ").is_err());
}

#[test]
fn test_format_days() {
    assert_eq!(format_duration(Duration::from_secs(86400)), "1d");
    assert_eq!(format_duration(Duration::from_secs(14 * 86400)), "14d");
}

#[test]
fn test_format_hours() {
    assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
    assert_eq!(format_duration(Duration::from_secs(12 * 3600)), "12h");
}

#[test]
fn test_format_minutes() {
    assert_eq!(format_duration(Duration::from_secs(60)), "1m");
    assert_eq!(format_duration(Duration::from_secs(30 * 60)), "30m");
}

#[test]
fn test_format_seconds() {
    assert_eq!(format_duration(Duration::from_secs(1)), "1s");
    assert_eq!(format_duration(Duration::from_secs(45)), "45s");
}

#[test]
fn test_format_zero() {
    assert_eq!(format_duration(Duration::from_secs(0)), "0s");
}

#[test]
fn test_format_non_divisible() {
    // 90 seconds = 1m 30s, formats as seconds since not evenly divisible
    assert_eq!(format_duration(Duration::from_secs(90)), "90s");
    // 3700 seconds = 1h 1m 40s, formats as seconds
    assert_eq!(format_duration(Duration::from_secs(3700)), "3700s");
}

#[test]
fn test_roundtrip() {
    // Test that parsing formatted output returns the same duration
    let durations = [
        Duration::from_secs(86400),      // 1d
        Duration::from_secs(14 * 86400), // 14d
        Duration::from_secs(3600),       // 1h
        Duration::from_secs(24 * 3600),  // 24h
        Duration::from_secs(60),         // 1m
        Duration::from_secs(30 * 60),    // 30m
        Duration::from_secs(1),          // 1s
        Duration::from_secs(45),         // 45s
    ];

    for d in durations {
        let formatted = format_duration(d);
        let parsed = parse_duration(&formatted).unwrap();
        assert_eq!(d, parsed, "Roundtrip failed for {d:?}");
    }
}

#[test]
fn test_serde_deserialize() {
    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(deserialize_with = "deserialize_duration")]
        timeout: Duration,
    }

    let config: TestConfig = toml::from_str(r#"timeout = "24h""#).unwrap();
    assert_eq!(config.timeout, Duration::from_secs(24 * 3600));
}

#[test]
fn test_serde_deserialize_opt_some() {
    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(default, deserialize_with = "deserialize_duration_opt")]
        timeout: Option<Duration>,
    }

    let config: TestConfig = toml::from_str(r#"timeout = "24h""#).unwrap();
    assert_eq!(config.timeout, Some(Duration::from_secs(24 * 3600)));
}

#[test]
fn test_serde_deserialize_opt_none() {
    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(default, deserialize_with = "deserialize_duration_opt")]
        timeout: Option<Duration>,
    }

    let config: TestConfig = toml::from_str(r#""#).unwrap();
    assert_eq!(config.timeout, None);
}
