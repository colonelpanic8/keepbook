use dioxus::prelude::*;

/// Size of a [`Spinner`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum SpinnerSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// An indeterminate activity spinner; pair it with text saying what is happening.
///
/// Mirror: `design/src/feedback/Spinner.tsx`.
#[component]
pub(crate) fn Spinner(size: Option<SpinnerSize>) -> Element {
    let class_name = match size.unwrap_or_default() {
        SpinnerSize::Small => "activity-spinner control-spinner",
        SpinnerSize::Medium => "activity-spinner",
        SpinnerSize::Large => "activity-spinner large",
    };
    rsx! {
        span { class: class_name, aria_hidden: "true" }
    }
}
