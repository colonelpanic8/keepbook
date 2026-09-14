//! View and selection state shared by the TUI's input, render, and data layers.

use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use ratatui::widgets::TableState;

use crate::app::transaction_tag_rules::TransactionTagMatcher;
use crate::app::{HistoryPoint, TransactionOutput};

use super::display::{
    compare_transactions, net_worth_point_date, net_worth_timestamp_sort_key, transaction_date,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiView {
    Transactions,
    NetWorth,
}

impl TuiView {
    pub(super) fn toggle(self) -> Self {
        match self {
            Self::Transactions => Self::NetWorth,
            Self::NetWorth => Self::Transactions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetWorthInterval {
    Full,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl NetWorthInterval {
    pub(super) const ALL: [Self; 6] = [
        Self::Full,
        Self::Hourly,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    pub(super) fn as_granularity(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    pub(super) fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub(super) fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TuiOptions {
    pub start_view: TuiView,
    pub net_worth_interval: NetWorthInterval,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            start_view: TuiView::Transactions,
            net_worth_interval: NetWorthInterval::Daily,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimeSpan {
    Days7,
    Days30,
    Days90,
    Days365,
    All,
}

impl TimeSpan {
    pub(super) const ALL: [Self; 5] = [
        Self::Days7,
        Self::Days30,
        Self::Days90,
        Self::Days365,
        Self::All,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Days7 => "7d",
            Self::Days30 => "30d",
            Self::Days90 => "90d",
            Self::Days365 => "365d",
            Self::All => "all",
        }
    }

    pub(super) fn cutoff_date(self, today: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days7 => Some(today - chrono::Duration::days(7)),
            Self::Days30 => Some(today - chrono::Duration::days(30)),
            Self::Days90 => Some(today - chrono::Duration::days(90)),
            Self::Days365 => Some(today - chrono::Duration::days(365)),
            Self::All => None,
        }
    }

    pub(super) fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub(super) fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SortMode {
    DateDesc,
    DateAsc,
    AmountAsc,
    AmountDesc,
}

impl SortMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::DateDesc => "date desc",
            Self::DateAsc => "date asc",
            Self::AmountAsc => "amount asc",
            Self::AmountDesc => "amount desc",
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::AmountAsc,
            Self::AmountAsc => Self::AmountDesc,
            Self::AmountDesc => Self::DateDesc,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SelectedTransactionInfo {
    pub(super) account_id: String,
    pub(super) account_name: String,
    pub(super) transaction_id: String,
    pub(super) status: String,
    pub(super) amount: String,
    pub(super) description: String,
}

impl SelectedTransactionInfo {
    pub(super) fn from_output(tx: &TransactionOutput) -> Self {
        Self {
            account_id: tx.account_id.clone(),
            account_name: tx.account_name.clone(),
            transaction_id: tx.id.clone(),
            status: tx.status.clone(),
            amount: tx.amount.clone(),
            description: tx.description.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum TagAction {
    OneOff { source: SelectedTransactionInfo },
    Rule { source: SelectedTransactionInfo },
}

#[derive(Debug, Clone)]
pub(super) struct TagModalState {
    pub(super) action: TagAction,
    pub(super) input: String,
    pub(super) cursor: usize,
    pub(super) suggestions: Vec<String>,
    pub(super) selected_suggestion: usize,
    pub(super) selection_active: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RegexModalState {
    pub(super) source: SelectedTransactionInfo,
    pub(super) tag: String,
    pub(super) input: String,
    pub(super) cursor: usize,
    pub(super) used_llm_suggestion: bool,
}

#[derive(Debug, Clone)]
pub(super) enum ModalState {
    Tag(TagModalState),
    Regex(RegexModalState),
}

pub(super) struct AppState {
    pub(super) active_view: TuiView,
    pub(super) all_transactions: Vec<TransactionOutput>,
    pub(super) visible_transaction_indices: Vec<usize>,
    pub(super) tag_matcher: TransactionTagMatcher,
    pub(super) tag_rules_path: PathBuf,
    pub(super) net_worth_points: Vec<HistoryPoint>,
    pub(super) visible_net_worth_indices: Vec<usize>,
    pub(super) net_worth_interval: NetWorthInterval,
    pub(super) net_worth_loaded: bool,
    pub(super) net_worth_error: Option<String>,
    pub(super) span: TimeSpan,
    pub(super) sort: SortMode,
    pub(super) include_ignored: bool,
    pub(super) transaction_last_refresh_utc: DateTime<Utc>,
    pub(super) net_worth_last_refresh_utc: DateTime<Utc>,
    pub(super) status_message: Option<String>,
    pub(super) modal: Option<ModalState>,
}

impl AppState {
    pub(super) fn new(
        all_transactions: Vec<TransactionOutput>,
        tag_matcher: TransactionTagMatcher,
        tag_rules_path: PathBuf,
        include_ignored: bool,
        options: TuiOptions,
    ) -> Self {
        let now = Utc::now();
        let mut state = Self {
            active_view: options.start_view,
            all_transactions,
            visible_transaction_indices: Vec::new(),
            tag_matcher,
            tag_rules_path,
            net_worth_points: Vec::new(),
            visible_net_worth_indices: Vec::new(),
            net_worth_interval: options.net_worth_interval,
            net_worth_loaded: false,
            net_worth_error: None,
            span: TimeSpan::Days30,
            sort: SortMode::DateDesc,
            include_ignored,
            transaction_last_refresh_utc: now,
            net_worth_last_refresh_utc: now,
            status_message: None,
            modal: None,
        };
        state.recompute_visible_transactions();
        state.recompute_visible_net_worth();
        state
    }

    pub(super) fn recompute_visible_transactions(&mut self) {
        let today = Utc::now().date_naive();
        let cutoff = self.span.cutoff_date(today);
        self.visible_transaction_indices = (0..self.all_transactions.len())
            .filter(|idx| match cutoff {
                Some(cutoff_date) => transaction_date(&self.all_transactions[*idx])
                    .map(|d| d >= cutoff_date)
                    .unwrap_or(true),
                None => true,
            })
            .collect();

        self.visible_transaction_indices
            .sort_unstable_by(|left_idx, right_idx| {
                compare_transactions(
                    &self.all_transactions[*left_idx],
                    &self.all_transactions[*right_idx],
                    self.sort,
                )
            });
    }

    pub(super) fn recompute_visible_net_worth(&mut self) {
        let today = Utc::now().date_naive();
        let cutoff = self.span.cutoff_date(today);
        self.visible_net_worth_indices = (0..self.net_worth_points.len())
            .filter(|idx| match cutoff {
                Some(cutoff_date) => net_worth_point_date(&self.net_worth_points[*idx])
                    .map(|d| d >= cutoff_date)
                    .unwrap_or(true),
                None => true,
            })
            .collect();

        self.visible_net_worth_indices
            .sort_unstable_by(|left_idx, right_idx| {
                let left = &self.net_worth_points[*left_idx];
                let right = &self.net_worth_points[*right_idx];
                net_worth_timestamp_sort_key(left)
                    .cmp(&net_worth_timestamp_sort_key(right))
                    .reverse()
            });
    }

    pub(super) fn visible_row_count(&self) -> usize {
        match self.active_view {
            TuiView::Transactions => self.visible_transaction_indices.len(),
            TuiView::NetWorth => self.visible_net_worth_indices.len(),
        }
    }
}

pub(super) fn selected_transaction<'a>(
    app_state: &'a AppState,
    tx_table_state: &TableState,
) -> Option<&'a TransactionOutput> {
    let selected_visible = tx_table_state.selected()?;
    let tx_index = app_state
        .visible_transaction_indices
        .get(selected_visible)?;
    app_state.all_transactions.get(*tx_index)
}

pub(super) fn select_transaction_by_id(
    app_state: &AppState,
    tx_table_state: &mut TableState,
    account_id: &str,
    transaction_id: &str,
) {
    let selected = app_state
        .visible_transaction_indices
        .iter()
        .position(|idx| {
            app_state
                .all_transactions
                .get(*idx)
                .map(|tx| tx.account_id == account_id && tx.id == transaction_id)
                .unwrap_or(false)
        });
    if let Some(index) = selected {
        tx_table_state.select(Some(index));
    }
}

pub(super) fn clamp_selection(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let selected = table_state.selected().unwrap_or(0);
    let clamped = selected.min(visible_len.saturating_sub(1));
    table_state.select(Some(clamped));
}

pub(super) fn select_prev(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let next = table_state.selected().unwrap_or(0).saturating_sub(1);
    table_state.select(Some(next));
}

pub(super) fn select_next(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let current = table_state.selected().unwrap_or(0);
    let max_index = visible_len.saturating_sub(1);
    table_state.select(Some((current + 1).min(max_index)));
}

pub(super) fn active_table_state_mut<'a>(
    active_view: TuiView,
    tx_table_state: &'a mut TableState,
    net_worth_table_state: &'a mut TableState,
) -> &'a mut TableState {
    match active_view {
        TuiView::Transactions => tx_table_state,
        TuiView::NetWorth => net_worth_table_state,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui/state_tests.rs"]
mod state_tests;
