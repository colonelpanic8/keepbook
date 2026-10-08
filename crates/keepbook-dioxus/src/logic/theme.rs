use material_colors::blend::harmonize;
use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Platform, SpecVersion, Variant};
use material_colors::hct::Hct;
use material_colors::palette::TonalPalette;
use material_colors::scheme::Scheme;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::sync::LazyLock;

/// A palette or mode as listed in `assets/themes.json`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct ThemeChoice {
    pub(crate) id: String,
    pub(crate) label: String,
    /// Generated at runtime from a seed color instead of defined in `styles.css`.
    #[serde(default)]
    pub(crate) generated: bool,
}

#[derive(Deserialize)]
pub(crate) struct ThemeCatalog {
    pub(crate) palettes: Vec<ThemeChoice>,
    pub(crate) modes: Vec<ThemeChoice>,
}

/// Palettes and modes in picker order, shared with the React mirror. Each
/// `<palette>-<light|dark>` pair is a `[data-theme]` block in `styles.css`,
/// except generated palettes.
pub(crate) static THEMES: LazyLock<ThemeCatalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/themes.json"))
        .expect("assets/themes.json should list palettes and modes")
});

/// The Material You palette, generated from a seed color.
pub(crate) const DYNAMIC_PALETTE: &str = "dynamic";
/// The seed for the dynamic palette until the user picks one: Fern's primary.
pub(crate) const DEFAULT_SEED: u32 = 0x1f6f8b;

/// The theme setting stored by `assets/theme.js`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ThemeSettings {
    pub(crate) palette: String,
    /// `light`, `dark`, or `system`.
    pub(crate) mode: String,
    /// Seed color for the dynamic palette, as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) seed: Option<String>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            palette: "fern".to_string(),
            mode: "system".to_string(),
            seed: None,
        }
    }
}

impl ThemeSettings {
    /// The setting with any unknown palette or mode replaced by the default.
    pub(crate) fn validated(self) -> Self {
        let defaults = Self::default();
        let known =
            |choices: &[ThemeChoice], id: &str| choices.iter().any(|choice| choice.id == id);
        Self {
            palette: if known(&THEMES.palettes, &self.palette) {
                self.palette
            } else {
                defaults.palette
            },
            mode: if known(&THEMES.modes, &self.mode) {
                self.mode
            } else {
                defaults.mode
            },
            seed: self.seed.filter(|seed| parse_hex_color(seed).is_some()),
        }
    }

    pub(crate) fn seed_color(&self) -> u32 {
        self.seed
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(DEFAULT_SEED)
    }
}

/// Parses `#rrggbb`.
pub(crate) fn parse_hex_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

pub(crate) fn hex_color(color: u32) -> String {
    format!("#{:06x}", color & 0xff_ffff)
}

/// keepbook's gain, loss, and warning hues, harmonized toward the seed.
const STATUS_COLORS: [u32; 3] = [0x1f7a4c, 0xbf3d3d, 0xb7831f];
/// Status backgrounds stay pale tints, as in the fixed palettes, rather than
/// Material's full-chroma containers.
const STATUS_BG_CHROMA: f64 = 12.0;

/// The `[data-theme="dynamic-light"]` and `[data-theme="dynamic-dark"]` blocks
/// for a seed color: Material You's tonal-spot scheme mapped onto keepbook's
/// color tokens.
pub(crate) fn dynamic_theme_css(seed: u32) -> String {
    let source = Rgb::from_u32(seed);
    let status = STATUS_COLORS.map(|color| {
        let hct: Hct = harmonize(Rgb::from_u32(color), source).into();
        (
            TonalPalette::of(hct.get_hue(), hct.get_chroma()),
            TonalPalette::of(hct.get_hue(), hct.get_chroma().min(STATUS_BG_CHROMA)),
        )
    });
    let mut css = String::new();
    for dark in [false, true] {
        let dynamic = DynamicScheme::from_spec(
            source.into(),
            Variant::TonalSpot,
            dark,
            None,
            Platform::Phone,
            SpecVersion::Spec2021,
        );
        let (line, color, container) = if dark { (40, 80, 25) } else { (80, 40, 94) };
        let extra = DynamicExtras {
            // A step past the surface tones, short of Material's outline.
            accent_border: dynamic.secondary_palette.tone(line),
            border_strong: dynamic.neutral_variant_palette.tone(line),
            status: status.map(|(text, bg)| (text.tone(color), bg.tone(container))),
        };
        write_dynamic_block(&mut css, &Scheme::from(dynamic), &extra, dark);
    }
    css
}

/// Tokens with no Material scheme role.
struct DynamicExtras {
    accent_border: Rgb,
    border_strong: Rgb,
    /// Gain, loss, and warning: each a color and its background.
    status: [(Rgb, Rgb); 3],
}

fn write_dynamic_block(css: &mut String, s: &Scheme, extra: &DynamicExtras, dark: bool) {
    let hex = |color: Rgb| format!("#{}", color.as_hex());
    // Light pages sit a step below their white cards; dark cards sit a step above the page.
    let (bg, surface, subtle, inset) = if dark {
        (
            s.surface,
            s.surface_container,
            s.surface_container_high,
            s.surface_container_highest,
        )
    } else {
        (
            s.surface_container_low,
            s.surface_container_lowest,
            s.surface,
            s.surface_container,
        )
    };
    let (divider, border) = if dark {
        (s.surface_container_highest, s.outline_variant)
    } else {
        (s.surface_container_high, s.surface_container_highest)
    };
    let [(positive, positive_bg), (negative, negative_bg), (warning, warning_bg)] = extra.status;
    let tokens = [
        ("bg", bg),
        ("surface", surface),
        ("surface-subtle", subtle),
        ("surface-inset", inset),
        ("text", s.on_surface),
        ("text-muted", s.on_surface_variant),
        ("on-primary", s.on_primary),
        ("divider", divider),
        ("border", border),
        ("border-strong", extra.border_strong),
        ("primary", s.primary),
        ("accent-bg", s.secondary_container),
        ("accent-border", extra.accent_border),
        ("accent-fg", s.on_secondary_container),
        ("positive", positive),
        ("positive-bg", positive_bg),
        ("negative", negative),
        ("negative-bg", negative_bg),
        ("warning", warning),
        ("warning-bg", warning_bg),
        ("tooltip-bg", s.inverse_surface),
        ("tooltip-fg", s.inverse_on_surface),
    ];
    let mode = if dark { "dark" } else { "light" };
    // `:root[...]` outranks the stylesheet's `:root` defaults on <html>, which
    // may be inserted after this block.
    let id = format!("{DYNAMIC_PALETTE}-{mode}");
    let _ = writeln!(css, "[data-theme=\"{id}\"],\n:root[data-theme=\"{id}\"] {{");
    let _ = writeln!(css, "  color-scheme: {mode};");
    for (name, color) in tokens {
        let _ = writeln!(css, "  --color-{name}: {};", hex(color));
    }
    let (scrim, shadow_sm, shadow_lg) = if dark {
        ("rgb(0 0 0 / 50%)", "rgb(0 0 0 / 40%)", "rgb(0 0 0 / 45%)")
    } else {
        ("rgb(0 0 0 / 32%)", "rgb(0 0 0 / 16%)", "rgb(0 0 0 / 18%)")
    };
    let _ = writeln!(css, "  --scrim: {scrim};");
    let _ = writeln!(css, "  --shadow-sm: 0 1px 3px {shadow_sm};");
    let _ = writeln!(css, "  --shadow-lg: 0 16px 40px {shadow_lg};");
    let _ = writeln!(css, "}}");
}
