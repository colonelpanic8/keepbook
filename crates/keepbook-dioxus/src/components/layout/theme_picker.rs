use crate::components::{InputKind, SegmentedControl, SegmentedOption, SettingRow, TextInput};
use crate::logic::{
    dynamic_theme_css, hex_color, ThemeChoice, ThemeSettings, DYNAMIC_PALETTE, THEMES,
};
use crate::platform::wallpaper_seed;
use crate::THEME_JS;
use dioxus::prelude::*;

fn segment_options(choices: &[ThemeChoice]) -> Vec<SegmentedOption> {
    choices
        .iter()
        .map(|choice| SegmentedOption::new(choice.id.clone(), choice.label.clone()))
        .collect()
}

/// The Settings theme rows: palette, mode, and the dynamic palette's seed color.
///
/// `wallpaper` means the system supplies the seed (Android 12 and later), so
/// there is no seed picker. Mirror: `design/src/layout/ThemeOptions.tsx`.
#[component]
pub(crate) fn ThemeOptions(
    settings: ThemeSettings,
    wallpaper: bool,
    onchange: EventHandler<ThemeSettings>,
) -> Element {
    let dynamic = settings.palette == DYNAMIC_PALETTE;
    let description = match (dynamic, wallpaper) {
        (true, true) => "Material You colors from your wallpaper",
        (true, false) => "Material You colors from a seed color",
        (false, _) => "Colors of the app",
    };
    let seed = hex_color(settings.seed_color());
    let for_palette = settings.clone();
    let for_mode = settings.clone();
    let for_seed = settings.clone();

    rsx! {
        SettingRow { title: "Theme", description, stacked: true,
            SegmentedControl {
                class: "setting-segmented".to_string(),
                label: "Theme".to_string(),
                options: segment_options(&THEMES.palettes),
                selected: settings.palette.clone(),
                onselect: move |palette: String| {
                    onchange.call(ThemeSettings { palette, ..for_palette.clone() })
                },
            }
        }
        SettingRow {
            title: "Mode",
            description: "Light, dark, or matching the system",
            stacked: true,
            SegmentedControl {
                class: "setting-segmented".to_string(),
                label: "Mode".to_string(),
                options: segment_options(&THEMES.modes),
                selected: settings.mode.clone(),
                onselect: move |mode: String| onchange.call(ThemeSettings { mode, ..for_mode.clone() }),
            }
        }
        if dynamic && !wallpaper {
            SettingRow {
                title: "Seed color",
                description: "The dynamic palette is generated from it",
                TextInput {
                    label: "Seed",
                    kind: InputKind::Color,
                    value: seed,
                    oninput: move |seed: String| {
                        onchange.call(ThemeSettings { seed: Some(seed), ..for_seed.clone() })
                    },
                }
            }
        }
    }
}

/// The theme rows bound to the stored setting: a change re-themes the app and
/// is remembered.
///
/// Mirror: `design/src/layout/ThemePicker.tsx`.
#[component]
pub(crate) fn ThemePicker() -> Element {
    let wallpaper = use_hook(|| wallpaper_seed().is_some());
    let mut settings = use_signal(ThemeSettings::default);
    use_future(move || async move {
        if let Some(stored) = read_theme_settings().await {
            settings.set(stored);
        }
    });

    rsx! {
        ThemeOptions {
            settings: settings(),
            wallpaper,
            onchange: move |next: ThemeSettings| {
                let next = next.validated();
                save_theme_settings(&next);
                settings.set(next);
            },
        }
    }
}

/// Regenerates the dynamic palette when the app starts, so it follows a seed
/// or wallpaper that changed since its CSS was stored.
pub(crate) fn use_dynamic_theme_refresh() {
    use_future(|| async {
        if let Some(stored) = read_theme_settings().await {
            if stored.palette == DYNAMIC_PALETTE {
                save_theme_settings(&stored);
            }
        }
    });
}

/// The setting stored by `assets/theme.js`.
async fn read_theme_settings() -> Option<ThemeSettings> {
    // The runtime installs itself once, so running it first covers callers
    // that start before its script tag has run.
    let stored = document::eval(&format!("{THEME_JS}\nreturn window.keepbookTheme.read();"))
        .await
        .ok()?;
    serde_json::from_value::<ThemeSettings>(stored)
        .ok()
        .map(ThemeSettings::validated)
}

/// Stores and applies a setting, generating the dynamic palette's CSS.
fn save_theme_settings(settings: &ThemeSettings) {
    let css = (settings.palette == DYNAMIC_PALETTE)
        .then(|| dynamic_theme_css(wallpaper_seed().unwrap_or_else(|| settings.seed_color())));
    // Both are JSON, so they are safe to embed as JavaScript literals.
    let settings = serde_json::to_string(settings).unwrap_or_default();
    let css = serde_json::to_string(&css).unwrap_or_else(|_| "null".to_string());
    let _ = document::eval(&format!(
        "{THEME_JS}\nwindow.keepbookTheme.write({settings}, {css});"
    ));
}
