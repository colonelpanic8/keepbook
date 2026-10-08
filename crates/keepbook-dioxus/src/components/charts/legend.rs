use dioxus::prelude::*;

/// One entry in a [`Legend`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegendItem {
    pub(crate) label: String,
    /// A CSS color, usually `var(--series-N)`.
    pub(crate) color: String,
    /// De-emphasized, e.g. an asset series beside account series.
    pub(crate) muted: bool,
}

/// A wrapping row of swatch and label pairs under a chart.
///
/// With `onselect`, entries are buttons and `selected` highlights one. Mirror:
/// `design/src/charts/Legend.tsx`.
#[component]
pub(crate) fn Legend(
    items: Vec<LegendItem>,
    class: Option<String>,
    selected: Option<String>,
    onselect: Option<EventHandler<String>>,
) -> Element {
    let class_name = match class {
        Some(extra) => format!("stacked-legend {extra}"),
        None => "stacked-legend".to_string(),
    };
    rsx! {
        div { class: "{class_name}",
            for item in items {
                {
                    let mut item_class = String::from("stacked-legend-item");
                    if item.muted {
                        item_class.push_str(" asset");
                    }
                    if selected.as_deref() == Some(item.label.as_str()) {
                        item_class.push_str(" selected");
                    }
                    let swatch = rsx! {
                        span { class: "stacked-legend-swatch", style: "background: {item.color};" }
                        span { "{item.label}" }
                    };
                    match onselect {
                        Some(handler) => {
                            let label = item.label.clone();
                            rsx! {
                                button {
                                    key: "{item.label}",
                                    class: "{item_class}",
                                    onclick: move |_| handler.call(label.clone()),
                                    {swatch}
                                }
                            }
                        }
                        None => rsx! {
                            span { key: "{item.label}", class: "{item_class}", {swatch} }
                        },
                    }
                }
            }
        }
    }
}
