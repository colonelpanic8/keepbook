use dioxus::prelude::*;

/// keepbook's data table: an uppercase header row above the given rows.
///
/// `class` names the table (e.g. `account-table`), whose stylesheet rule sets
/// the column tracks. Rows are `div.table-row` children. Mirror:
/// `design/src/display/DataTable.tsx`.
#[component]
pub(crate) fn DataTable(class: Option<String>, columns: Vec<String>, children: Element) -> Element {
    let class_name = match class {
        Some(extra) => format!("data-table {extra}"),
        None => "data-table".to_string(),
    };
    rsx! {
        div { class: "{class_name}",
            div { class: "table-head",
                for column in columns {
                    span { "{column}" }
                }
            }
            {children}
        }
    }
}
