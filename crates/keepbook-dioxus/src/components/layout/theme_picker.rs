use crate::components::{SegmentedControl, SegmentedOption, SettingRow};
use dioxus::prelude::*;
use serde::Deserialize;
use std::sync::LazyLock;

/// A selectable theme, as listed in `assets/themes.json`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Theme {
    pub(crate) id: String,
    pub(crate) label: String,
}

/// The app's themes, in picker order, shared with the React mirror. Each `id`
/// is a `[data-theme]` block in `styles.css`; `fern` is the `:root` default.
pub(crate) static THEMES: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/themes.json"))
        .expect("assets/themes.json should list the themes")
});

const STORAGE_KEY: &str = "keepbook-theme";

/// The Settings theme row: picking a theme re-themes the app and remembers it.
///
/// Mirror: `design/src/layout/ThemePicker.tsx`.
#[component]
pub(crate) fn ThemePicker() -> Element {
    let mut current = use_signal(|| "fern".to_string());
    use_future(move || async move {
        let stored = document::eval(&format!("return localStorage.getItem({STORAGE_KEY:?});"));
        if let Ok(value) = stored.await {
            if let Some(theme) = value.as_str() {
                if THEMES.iter().any(|t| t.id == theme) {
                    current.set(theme.to_string());
                }
            }
        }
    });

    rsx! {
        SettingRow {
            title: "Theme",
            description: "Appearance of the app",
            stacked: true,
            SegmentedControl {
                class: "setting-segmented".to_string(),
                label: "Theme".to_string(),
                options: THEMES
                    .iter()
                    .map(|t| SegmentedOption::new(t.id.clone(), t.label.clone()))
                    .collect::<Vec<_>>(),
                selected: current(),
                onselect: move |theme: String| {
                    if !THEMES.iter().any(|t| t.id == theme) {
                        return;
                    }
                    current.set(theme.clone());
                    // `theme` was just matched against THEMES, so formatting it into JS is safe.
                    let _ = document::eval(&format!(
                        r#"
                        var theme = {theme:?};
                        if (theme === "fern") {{
                            delete document.documentElement.dataset.theme;
                        }} else {{
                            document.documentElement.dataset.theme = theme;
                        }}
                        localStorage.setItem({STORAGE_KEY:?}, theme);
                        "#
                    ));
                },
            }
        }
    }
}
