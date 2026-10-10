use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Path data for every icon, shared with the React mirror.
static ICON_PATHS: LazyLock<HashMap<String, Vec<String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../assets/icons.json"))
        .expect("assets/icons.json should be a map of icon names to SVG paths")
});

/// Stroke icons drawn on a 24x24 grid (Lucide shapes), named as in `assets/icons.json`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ButtonIcon {
    Refresh,
    ChevronDown,
    Check,
    CircleAlert,
    Wallet,
    Landmark,
    Receipt,
    Repeat,
    TrendingUp,
    Layers,
    Plug,
    FilePen,
    Settings,
}

impl ButtonIcon {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::ChevronDown => "chevron-down",
            Self::Check => "check",
            Self::CircleAlert => "circle-alert",
            Self::Wallet => "wallet",
            Self::Landmark => "landmark",
            Self::Receipt => "receipt",
            Self::Repeat => "repeat",
            Self::TrendingUp => "trending-up",
            Self::Layers => "layers",
            Self::Plug => "plug",
            Self::FilePen => "file-pen-line",
            Self::Settings => "settings",
        }
    }

    pub(crate) fn paths(self) -> &'static [String] {
        ICON_PATHS
            .get(self.name())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

/// A 14px stroke icon in the current text color; CSS may size it up.
///
/// Mirror: `renderIcon` in `design/src/actions/icons.tsx`.
#[component]
pub(crate) fn IconSvg(icon: ButtonIcon) -> Element {
    rsx! {
        svg {
            class: "button-icon",
            view_box: "0 0 24 24",
            width: "14",
            height: "14",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            for d in icon.paths() {
                path { d: "{d}" }
            }
        }
    }
}
