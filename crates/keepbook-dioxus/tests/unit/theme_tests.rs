//! Every theme is complete, readable, and listed in `assets/themes.json`.

use crate::logic::{
    dynamic_theme_css, parse_hex_color, ThemeSettings, DEFAULT_SEED, DYNAMIC_PALETTE, THEMES,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

type Tokens = BTreeMap<String, String>;

/// Seeds across the hue wheel plus extremes, for the generated palette.
const SEEDS: [u32; 12] = [
    DEFAULT_SEED,
    0x6750a4,
    0xff0000,
    0xffeb3b,
    0x00c853,
    0x2962ff,
    0xaa00ff,
    0xff6d00,
    0x795548,
    0x9e9e9e,
    0x000000,
    0xffffff,
];

/// Foregrounds and the backgrounds they are read on.
const READABLE_PAIRS: &[(&str, &[&str])] = &[
    (
        "--color-text",
        &[
            "--color-bg",
            "--color-surface",
            "--color-surface-subtle",
            "--color-surface-inset",
        ],
    ),
    (
        "--color-text-muted",
        &[
            "--color-bg",
            "--color-surface",
            "--color-surface-subtle",
            "--color-surface-inset",
        ],
    ),
    ("--color-primary", &["--color-bg", "--color-surface"]),
    ("--color-on-primary", &["--color-primary"]),
    ("--color-accent-fg", &["--color-accent-bg"]),
    (
        "--color-positive",
        &["--color-positive-bg", "--color-surface"],
    ),
    (
        "--color-negative",
        &["--color-negative-bg", "--color-surface"],
    ),
    (
        "--color-warning",
        &["--color-warning-bg", "--color-surface"],
    ),
    ("--color-tooltip-fg", &["--color-tooltip-bg"]),
];

fn without_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start..]
            .find("*/")
            .map_or("", |end| &rest[start + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// Declarations per `data-theme` value, merged across every rule naming it.
fn theme_tokens(css: &str) -> BTreeMap<String, Tokens> {
    let css = without_comments(css);
    let mut themes = BTreeMap::<String, Tokens>::new();
    let mut rest = css.as_str();
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        // Text after a nested block's closing brace is not part of this selector.
        let selectors = rest[..open].rsplit('}').next().unwrap_or_default();
        let body = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        for selector in selectors.split(',') {
            let Some(id) = selector
                .trim()
                .strip_prefix("[data-theme=\"")
                .and_then(|id| id.strip_suffix("\"]"))
            else {
                continue;
            };
            let tokens = themes.entry(id.to_string()).or_default();
            for declaration in body.split(';') {
                if let Some((name, value)) = declaration.split_once(':') {
                    tokens.insert(name.trim().to_string(), value.trim().to_string());
                }
            }
        }
    }
    themes
}

/// The app's themes plus the generated palette for `seed`.
fn all_themes(seed: u32) -> BTreeMap<String, Tokens> {
    theme_tokens(&format!("{}\n{}", crate::APP_CSS, dynamic_theme_css(seed)))
}

fn luminance(hex: &str) -> f64 {
    let color = parse_hex_color(hex).unwrap_or_else(|| panic!("{hex} should be #rrggbb"));
    let channel = |shift: u32| {
        let value = f64::from((color >> shift) & 0xff) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn unreadable_pairs(theme: &str, tokens: &Tokens) -> Vec<String> {
    let mut failures = Vec::new();
    for (foreground, backgrounds) in READABLE_PAIRS {
        for background in *backgrounds {
            let ratio = contrast(&tokens[*foreground], &tokens[*background]);
            if ratio < 4.5 {
                failures.push(format!(
                    "{theme}: {foreground} on {background} is {ratio:.2}:1"
                ));
            }
        }
    }
    failures
}

#[test]
fn themes_json_lists_exactly_the_stylesheet_themes() {
    let listed = THEMES
        .palettes
        .iter()
        .filter(|palette| !palette.generated)
        .flat_map(|palette| ["light", "dark"].map(|mode| format!("{}-{mode}", palette.id)))
        .collect::<BTreeSet<_>>();
    let defined = theme_tokens(crate::APP_CSS)
        .into_keys()
        .filter(|id| !id.starts_with(&format!("{DYNAMIC_PALETTE}-")))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        defined, listed,
        "styles.css must define a light and a dark block for each palette in assets/themes.json"
    );
}

#[test]
fn every_theme_states_every_token() {
    let themes = all_themes(DEFAULT_SEED);
    let required = themes["fern-light"].keys().collect::<Vec<_>>();
    for (id, tokens) in &themes {
        assert_eq!(
            tokens.keys().collect::<Vec<_>>(),
            required,
            "{id} must state the same tokens as fern-light, so a themed container inherits none"
        );
        let mode = id.rsplit('-').next().unwrap_or_default();
        assert_eq!(
            tokens["color-scheme"], mode,
            "{id} must declare color-scheme: {mode}, or platform-drawn controls keep the other scheme"
        );
    }
}

#[test]
fn every_theme_is_readable() {
    let mut failures = Vec::new();
    for (id, tokens) in theme_tokens(crate::APP_CSS) {
        // The stylesheet only gives the generated palette its series colors.
        if !id.starts_with(&format!("{DYNAMIC_PALETTE}-")) {
            failures.extend(unreadable_pairs(&id, &tokens));
        }
    }
    for seed in SEEDS {
        for (id, tokens) in theme_tokens(&dynamic_theme_css(seed)) {
            failures.extend(unreadable_pairs(
                &format!("{id} (seed #{seed:06x})"),
                &tokens,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "text below WCAG AA contrast:\n{}",
        failures.join("\n")
    );
}

#[test]
fn stored_settings_fall_back_to_known_values() {
    let stored = ThemeSettings {
        palette: "missing".to_string(),
        mode: "dusk".to_string(),
        seed: Some("teal".to_string()),
    };
    assert_eq!(stored.validated(), ThemeSettings::default());

    let stored = ThemeSettings {
        palette: DYNAMIC_PALETTE.to_string(),
        mode: "dark".to_string(),
        seed: Some("#6750a4".to_string()),
    };
    assert_eq!(stored.clone().validated(), stored);
    assert_eq!(stored.seed_color(), 0x6750a4);
}

/// The design package shows the dynamic palette with this sample, so it never
/// reimplements the generator.
#[test]
fn dynamic_theme_sample_matches_the_generator() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("design/parity/dynamic-theme.css");
    let css = dynamic_theme_css(DEFAULT_SEED);
    if std::env::var_os("UPDATE_PARITY").is_some() {
        std::fs::write(&path, &css).expect("dynamic theme sample should be writable");
    }
    assert_eq!(
        std::fs::read_to_string(&path).ok().as_deref(),
        Some(css.as_str()),
        "run `UPDATE_PARITY=1 cargo test -p keepbook-dioxus` to refresh design/parity/dynamic-theme.css"
    );
}
