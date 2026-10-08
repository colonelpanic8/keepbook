use dioxus::prelude::*;

/// Tone of a [`Badge`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum BadgeTone {
    #[default]
    Neutral,
    Positive,
    Negative,
    Warning,
}

/// A short status label in a pill, neutral unless toned.
///
/// Mirror: `design/src/display/Badge.tsx`.
#[component]
pub(crate) fn Badge(tone: Option<BadgeTone>, children: Element) -> Element {
    let class_name = match tone.unwrap_or_default() {
        BadgeTone::Neutral => "badge",
        BadgeTone::Positive => "badge positive",
        BadgeTone::Negative => "badge negative",
        BadgeTone::Warning => "badge warning",
    };
    rsx! {
        span { class: class_name, {children} }
    }
}
