use dioxus::prelude::*;

/// A titled group of rows, such as a connection and its accounts.
///
/// Mirror: `design/src/display/TreeGroup.tsx`.
#[component]
pub(crate) fn TreeGroup(
    title: String,
    subtitle: Option<String>,
    aside: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        section { class: "tree-group",
            div { class: "tree-parent",
                div {
                    strong { "{title}" }
                    if let Some(subtitle) = subtitle {
                        small { "{subtitle}" }
                    }
                }
                {aside}
            }
            {children}
        }
    }
}
