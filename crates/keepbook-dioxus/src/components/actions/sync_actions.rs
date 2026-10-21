use super::{ButtonFeedback, ButtonIcon, MenuAction, SplitButton};
use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SyncAction {
    Balances {
        if_stale: bool,
        full_transactions: bool,
    },
    Prices {
        force: bool,
    },
}

impl SyncAction {
    pub(crate) fn feedback_key(self, target: Option<&str>) -> String {
        let control = match self {
            Self::Balances { .. } => "balances",
            _ => "prices",
        };
        format!("{control}:{}", target.unwrap_or("all"))
    }
}

#[component]
pub(crate) fn SyncActions(
    #[props(default)] disabled: bool,
    balances_feedback: Option<ButtonFeedback>,
    prices_feedback: Option<ButtonFeedback>,
    onaction: EventHandler<SyncAction>,
) -> Element {
    rsx! {
        div { class: "connection-actions",
            SplitButton {
                icon: ButtonIcon::Refresh,
                title: "Refresh balances and recent transactions for stale connections",
                menu_label: "More balance refresh options",
                disabled,
                feedback: balances_feedback,
                actions: vec![
                    MenuAction { value: "all", label: "Refresh all balances", detail: "Refresh even if balances are still fresh" },
                    MenuAction { value: "full", label: "Refresh full history", detail: "Refresh balances and all available transactions" },
                    MenuAction { value: "stale-full", label: "Refresh stale full history", detail: "Fetch all transactions only for stale connections" },
                ],
                onclick: move |_| onaction.call(SyncAction::Balances { if_stale: true, full_transactions: false }),
                onselect: move |value| match value {
                    "all" => onaction.call(SyncAction::Balances { if_stale: false, full_transactions: false }),
                    "full" => onaction.call(SyncAction::Balances { if_stale: false, full_transactions: true }),
                    "stale-full" => onaction.call(SyncAction::Balances { if_stale: true, full_transactions: true }),
                    _ => {},
                },
                "Balances"
            }
            SplitButton {
                icon: ButtonIcon::Refresh,
                title: "Refetch every price, ignoring staleness",
                menu_label: "More price refresh options",
                disabled,
                feedback: prices_feedback,
                actions: vec![MenuAction {
                    value: "stale-prices",
                    label: "Refresh stale prices",
                    detail: "Skip prices that are still fresh",
                }],
                onclick: move |_| onaction.call(SyncAction::Prices { force: true }),
                onselect: move |value| {
                    if value == "stale-prices" {
                        onaction.call(SyncAction::Prices { force: false });
                    }
                },
                "Prices"
            }
        }
    }
}
