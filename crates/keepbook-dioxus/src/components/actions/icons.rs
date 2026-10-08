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
    GitBranch,
    ChevronDown,
}

impl ButtonIcon {
    #[cfg(test)]
    pub(crate) const ALL: [ButtonIcon; 3] = [Self::Refresh, Self::GitBranch, Self::ChevronDown];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::GitBranch => "git-branch",
            Self::ChevronDown => "chevron-down",
        }
    }

    pub(crate) fn paths(self) -> &'static [String] {
        ICON_PATHS
            .get(self.name())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[component]
pub(super) fn IconSvg(icon: ButtonIcon) -> Element {
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
