use std::cmp::Ordering;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::{Frame, Terminal};
use regex::Regex;
use rust_decimal::Decimal;

use crate::app::transaction_tag_rules::{
    append_transaction_tag_rule, exact_ci_regex_pattern, fallback_regex_suggestion,
    load_transaction_tag_rules, suggest_regex_with_openai, tag_rules_path, TransactionTagMatcher,
    TransactionTagRule, TransactionTagRuleInput,
};
use crate::app::{self, HistoryPoint, TransactionOutput};
use crate::config::ResolvedConfig;
use crate::format::{currency_symbol, format_base_currency_display};
use crate::storage::Storage;

const LOAD_START_DATE: &str = "1900-01-01";
const LOAD_END_DATE: &str = "9999-12-31";
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiView {
    Transactions,
    NetWorth,
}

impl TuiView {
    fn toggle(self) -> Self {
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
    const ALL: [Self; 6] = [
        Self::Full,
        Self::Hourly,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    fn as_granularity(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
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
enum TimeSpan {
    Days7,
    Days30,
    Days90,
    Days365,
    All,
}

impl TimeSpan {
    const ALL: [Self; 5] = [
        Self::Days7,
        Self::Days30,
        Self::Days90,
        Self::Days365,
        Self::All,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Days7 => "7d",
            Self::Days30 => "30d",
            Self::Days90 => "90d",
            Self::Days365 => "365d",
            Self::All => "all",
        }
    }

    fn cutoff_date(self, today: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days7 => Some(today - chrono::Duration::days(7)),
            Self::Days30 => Some(today - chrono::Duration::days(30)),
            Self::Days90 => Some(today - chrono::Duration::days(90)),
            Self::Days365 => Some(today - chrono::Duration::days(365)),
            Self::All => None,
        }
    }

    fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortMode {
    DateDesc,
    DateAsc,
    AmountAsc,
    AmountDesc,
}

impl SortMode {
    fn label(self) -> &'static str {
        match self {
            Self::DateDesc => "date desc",
            Self::DateAsc => "date asc",
            Self::AmountAsc => "amount asc",
            Self::AmountDesc => "amount desc",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::AmountAsc,
            Self::AmountAsc => Self::AmountDesc,
            Self::AmountDesc => Self::DateDesc,
        }
    }
}

#[derive(Debug, Clone)]
struct SelectedTransactionInfo {
    account_id: String,
    account_name: String,
    transaction_id: String,
    status: String,
    amount: String,
    description: String,
}

impl SelectedTransactionInfo {
    fn from_output(tx: &TransactionOutput) -> Self {
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
enum TagAction {
    OneOff { source: SelectedTransactionInfo },
    Rule { source: SelectedTransactionInfo },
}

#[derive(Debug, Clone)]
struct TagModalState {
    action: TagAction,
    input: String,
    cursor: usize,
    suggestions: Vec<String>,
    selected_suggestion: usize,
    selection_active: bool,
}

#[derive(Debug, Clone)]
struct RegexModalState {
    source: SelectedTransactionInfo,
    tag: String,
    input: String,
    cursor: usize,
    used_llm_suggestion: bool,
}

#[derive(Debug, Clone)]
enum ModalState {
    Tag(TagModalState),
    Regex(RegexModalState),
}

struct AppState {
    active_view: TuiView,
    all_transactions: Vec<TransactionOutput>,
    visible_transaction_indices: Vec<usize>,
    tag_matcher: TransactionTagMatcher,
    tag_rules_path: PathBuf,
    net_worth_points: Vec<HistoryPoint>,
    visible_net_worth_indices: Vec<usize>,
    net_worth_interval: NetWorthInterval,
    net_worth_loaded: bool,
    net_worth_error: Option<String>,
    span: TimeSpan,
    sort: SortMode,
    include_ignored: bool,
    transaction_last_refresh_utc: DateTime<Utc>,
    net_worth_last_refresh_utc: DateTime<Utc>,
    status_message: Option<String>,
    modal: Option<ModalState>,
}

impl AppState {
    fn new(
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

    fn recompute_visible_transactions(&mut self) {
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

    fn recompute_visible_net_worth(&mut self) {
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

    fn visible_row_count(&self) -> usize {
        match self.active_view {
            TuiView::Transactions => self.visible_transaction_indices.len(),
            TuiView::NetWorth => self.visible_net_worth_indices.len(),
        }
    }
}

pub async fn run_tui(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    options: TuiOptions,
) -> Result<()> {
    let include_ignored = false;
    let transactions = load_transactions(storage.as_ref(), config, include_ignored).await?;
    let rules_path = tag_rules_path(&config.data_dir);
    let (tag_matcher, rule_warning) = load_transaction_tag_rules(&rules_path)?;
    let mut app_state = AppState::new(
        transactions,
        tag_matcher,
        rules_path,
        include_ignored,
        options,
    );
    app_state.status_message = rule_warning;
    if app_state.active_view == TuiView::NetWorth {
        refresh_net_worth(&mut app_state, storage.clone(), config).await;
    }

    let mut tx_table_state = TableState::default();
    tx_table_state.select(Some(0));
    let mut net_worth_table_state = TableState::default();
    net_worth_table_state.select(Some(0));

    let mut terminal = enter_terminal()?;
    let result = run_event_loop(
        &mut terminal,
        &mut app_state,
        &mut tx_table_state,
        &mut net_worth_table_state,
        storage,
        config,
    )
    .await;
    leave_terminal(&mut terminal)?;
    result
}

async fn refresh_transactions_and_rules(
    app_state: &mut AppState,
    storage: &dyn Storage,
    config: &ResolvedConfig,
) -> Result<()> {
    app_state.all_transactions =
        load_transactions(storage, config, app_state.include_ignored).await?;
    app_state.transaction_last_refresh_utc = Utc::now();
    app_state.recompute_visible_transactions();

    let (matcher, warning) = load_transaction_tag_rules(&app_state.tag_rules_path)?;
    app_state.tag_matcher = matcher;
    if warning.is_some() {
        app_state.status_message = warning;
    }
    Ok(())
}

fn selected_transaction<'a>(
    app_state: &'a AppState,
    tx_table_state: &TableState,
) -> Option<&'a TransactionOutput> {
    let selected_visible = tx_table_state.selected()?;
    let tx_index = app_state
        .visible_transaction_indices
        .get(selected_visible)?;
    app_state.all_transactions.get(*tx_index)
}

fn select_transaction_by_id(
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

#[derive(Debug, Clone, Copy)]
enum TagActionKind {
    OneOff,
    Rule,
}

fn collect_tag_catalog(app_state: &AppState) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut add = |raw: &str| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    };

    for rule in &app_state.tag_matcher.rules {
        if let Some(tag) = &rule.tag {
            add(tag);
        }
    }
    for tx in &app_state.all_transactions {
        if let Some(tag) = tx.tags.first().map(String::as_str) {
            add(tag);
        }
    }
    out.sort_by_key(|value| value.to_lowercase());
    out
}

fn filtered_tag_suggestions(catalog: &[String], input: &str) -> Vec<String> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        return catalog.to_vec();
    }

    let mut starts_with = Vec::new();
    let mut contains = Vec::new();
    for candidate in catalog {
        let candidate_lc = candidate.to_lowercase();
        if candidate_lc.starts_with(&trimmed) {
            starts_with.push(candidate.clone());
        } else if candidate_lc.contains(&trimmed) {
            contains.push(candidate.clone());
        }
    }
    starts_with.extend(contains);
    starts_with
}

fn refresh_tag_suggestions(modal: &mut TagModalState, catalog: &[String]) {
    modal.suggestions = filtered_tag_suggestions(catalog, &modal.input);
    if modal.suggestions.is_empty() {
        modal.selected_suggestion = 0;
    } else {
        modal.selected_suggestion = modal
            .selected_suggestion
            .min(modal.suggestions.len().saturating_sub(1));
    }
}

fn selected_tag_from_modal(modal: &TagModalState) -> String {
    if modal.selection_active {
        if let Some(choice) = modal.suggestions.get(modal.selected_suggestion) {
            return choice.clone();
        }
    }
    modal.input.clone()
}

fn text_char_len(input: &str) -> usize {
    input.chars().count()
}

fn char_to_byte_idx(input: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    input
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| input.len())
}

fn remove_char_at(input: &mut String, char_index: usize) -> bool {
    let len = text_char_len(input);
    if char_index >= len {
        return false;
    }
    let start = char_to_byte_idx(input, char_index);
    let end = char_to_byte_idx(input, char_index + 1);
    input.replace_range(start..end, "");
    true
}

fn clamp_cursor(input: &str, cursor: usize) -> usize {
    cursor.min(text_char_len(input))
}

fn apply_text_input_edit(input: &mut String, cursor: &mut usize, key: KeyCode) -> bool {
    *cursor = clamp_cursor(input, *cursor);
    match key {
        KeyCode::Backspace => {
            if *cursor == 0 {
                return false;
            }
            let removed = remove_char_at(input, *cursor - 1);
            if removed {
                *cursor -= 1;
            }
            removed
        }
        KeyCode::Delete => remove_char_at(input, *cursor),
        KeyCode::Left => {
            if *cursor > 0 {
                *cursor -= 1;
                return true;
            }
            false
        }
        KeyCode::Right => {
            let len = text_char_len(input);
            if *cursor < len {
                *cursor += 1;
                return true;
            }
            false
        }
        KeyCode::Home => {
            if *cursor != 0 {
                *cursor = 0;
                return true;
            }
            false
        }
        KeyCode::End => {
            let len = text_char_len(input);
            if *cursor != len {
                *cursor = len;
                return true;
            }
            false
        }
        KeyCode::Char(ch) => {
            let idx = char_to_byte_idx(input, *cursor);
            input.insert(idx, ch);
            *cursor += 1;
            true
        }
        _ => false,
    }
}

fn open_tag_modal_for_selected(
    app_state: &mut AppState,
    tx_table_state: &TableState,
    action_kind: TagActionKind,
) {
    let Some(selected) = selected_transaction(app_state, tx_table_state) else {
        app_state.status_message = Some("No transaction selected".to_string());
        return;
    };

    let default_input =
        resolved_transaction_tag(selected, &app_state.tag_matcher).unwrap_or_default();
    let source = SelectedTransactionInfo::from_output(selected);
    let action = match action_kind {
        TagActionKind::OneOff => TagAction::OneOff { source },
        TagActionKind::Rule => TagAction::Rule { source },
    };
    let catalog = collect_tag_catalog(app_state);
    let mut modal = TagModalState {
        action,
        input: default_input,
        cursor: 0,
        suggestions: Vec::new(),
        selected_suggestion: 0,
        selection_active: false,
    };
    modal.cursor = text_char_len(&modal.input);
    refresh_tag_suggestions(&mut modal, &catalog);
    app_state.modal = Some(ModalState::Tag(modal));
}

async fn handle_tag_modal_key(
    app_state: &mut AppState,
    tx_table_state: &mut TableState,
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    mut modal: TagModalState,
    key: KeyCode,
) -> Result<Option<ModalState>> {
    match key {
        KeyCode::Esc => {
            app_state.status_message = Some("Retagging canceled".to_string());
            Ok(None)
        }
        KeyCode::Up => {
            if !modal.suggestions.is_empty() {
                modal.selection_active = true;
                modal.selected_suggestion = modal.selected_suggestion.saturating_sub(1);
            }
            Ok(Some(ModalState::Tag(modal)))
        }
        KeyCode::Down => {
            if !modal.suggestions.is_empty() {
                modal.selection_active = true;
                let max = modal.suggestions.len().saturating_sub(1);
                modal.selected_suggestion = (modal.selected_suggestion + 1).min(max);
            }
            Ok(Some(ModalState::Tag(modal)))
        }
        KeyCode::Tab => {
            if let Some(choice) = modal.suggestions.get(modal.selected_suggestion) {
                modal.input = choice.clone();
                modal.cursor = text_char_len(&modal.input);
                modal.selection_active = false;
            }
            let catalog = collect_tag_catalog(app_state);
            refresh_tag_suggestions(&mut modal, &catalog);
            Ok(Some(ModalState::Tag(modal)))
        }
        KeyCode::Enter => {
            let chosen_tag = selected_tag_from_modal(&modal).trim().to_string();
            match modal.action.clone() {
                TagAction::OneOff { source } => {
                    let clear_tag = chosen_tag.is_empty();
                    let tags = if clear_tag {
                        Vec::new()
                    } else {
                        vec![chosen_tag]
                    };
                    app::set_transaction_annotation(
                        storage.as_ref(),
                        config,
                        &source.account_id,
                        &source.transaction_id,
                        app::TransactionAnnotationInput {
                            tags,
                            clear_tags: clear_tag,
                            ..Default::default()
                        },
                    )
                    .await?;
                    refresh_transactions_and_rules(app_state, storage.as_ref(), config).await?;
                    select_transaction_by_id(
                        app_state,
                        tx_table_state,
                        &source.account_id,
                        &source.transaction_id,
                    );
                    app_state.status_message = Some(format!(
                        "{} tag for {}",
                        if clear_tag { "Cleared" } else { "Updated" },
                        source.transaction_id
                    ));
                    Ok(None)
                }
                TagAction::Rule { source } => {
                    if chosen_tag.is_empty() {
                        app_state.status_message = Some("Rule tag cannot be empty".to_string());
                        return Ok(Some(ModalState::Tag(modal)));
                    }

                    let mut regex_suggestion = fallback_regex_suggestion(&source.description);
                    let mut used_llm_suggestion = false;
                    match suggest_regex_with_openai(
                        &chosen_tag,
                        &source.account_name,
                        &source.status,
                        &source.amount,
                        &source.description,
                    )
                    .await
                    {
                        Ok(Some(suggested)) => {
                            regex_suggestion = suggested;
                            used_llm_suggestion = true;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            app_state.status_message = Some(format!(
                                "LLM suggestion failed; using fallback regex ({error})"
                            ));
                        }
                    }

                    let cursor = text_char_len(&regex_suggestion);
                    Ok(Some(ModalState::Regex(RegexModalState {
                        source,
                        tag: chosen_tag,
                        input: regex_suggestion,
                        cursor,
                        used_llm_suggestion,
                    })))
                }
            }
        }
        _ => {
            if apply_text_input_edit(&mut modal.input, &mut modal.cursor, key) {
                modal.selection_active = false;
                let catalog = collect_tag_catalog(app_state);
                refresh_tag_suggestions(&mut modal, &catalog);
            }
            Ok(Some(ModalState::Tag(modal)))
        }
    }
}

async fn handle_regex_modal_key(
    app_state: &mut AppState,
    tx_table_state: &mut TableState,
    mut modal: RegexModalState,
    key: KeyCode,
) -> Result<Option<ModalState>> {
    match key {
        KeyCode::Esc => {
            app_state.status_message = Some("Rule creation canceled".to_string());
            Ok(None)
        }
        KeyCode::Enter => {
            let regex_pattern = modal.input.trim();
            if regex_pattern.is_empty() {
                app_state.status_message = Some("Regex cannot be empty".to_string());
                return Ok(Some(ModalState::Regex(modal)));
            }
            if let Err(error) = Regex::new(regex_pattern) {
                app_state.status_message = Some(format!("Invalid regex: {error}"));
                return Ok(Some(ModalState::Regex(modal)));
            }

            let rule = TransactionTagRule {
                set_description: None,
                set_tags: Some(vec![modal.tag.clone()]),
                set_subtags: None,
                match_account_id: None,
                match_account_name: exact_ci_regex_pattern(&modal.source.account_name),
                match_description: Some(regex_pattern.to_string()),
                match_tag: None,
                match_subtag: None,
                match_status: None,
                match_amount: None,
            };
            append_transaction_tag_rule(&app_state.tag_rules_path, &rule)?;
            let (matcher, warning) = load_transaction_tag_rules(&app_state.tag_rules_path)?;
            app_state.tag_matcher = matcher;
            if let Some(message) = warning {
                app_state.status_message = Some(message);
            } else {
                app_state.status_message = Some(format!(
                    "Added tag rule for {} ({})",
                    modal.source.transaction_id,
                    if modal.used_llm_suggestion {
                        "LLM suggestion"
                    } else {
                        "fallback suggestion"
                    }
                ));
            }
            select_transaction_by_id(
                app_state,
                tx_table_state,
                &modal.source.account_id,
                &modal.source.transaction_id,
            );
            Ok(None)
        }
        _ => {
            apply_text_input_edit(&mut modal.input, &mut modal.cursor, key);
            Ok(Some(ModalState::Regex(modal)))
        }
    }
}

async fn handle_modal_key(
    app_state: &mut AppState,
    tx_table_state: &mut TableState,
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    key: KeyCode,
) -> Result<bool> {
    let Some(modal_state) = app_state.modal.take() else {
        return Ok(false);
    };

    let next_modal = match modal_state {
        ModalState::Tag(modal) => {
            handle_tag_modal_key(app_state, tx_table_state, storage, config, modal, key).await?
        }
        ModalState::Regex(modal) => {
            handle_regex_modal_key(app_state, tx_table_state, modal, key).await?
        }
    };
    app_state.modal = next_modal;
    Ok(true)
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app_state: &mut AppState,
    tx_table_state: &mut TableState,
    net_worth_table_state: &mut TableState,
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    loop {
        let active_table_state =
            active_table_state_mut(app_state.active_view, tx_table_state, net_worth_table_state);
        clamp_selection(app_state.visible_row_count(), active_table_state);
        terminal.draw(|frame| {
            render(
                frame,
                app_state,
                tx_table_state,
                net_worth_table_state,
                config,
            )
        })?;

        if !event::poll(POLL_INTERVAL)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if handle_modal_key(app_state, tx_table_state, storage.clone(), config, key.code).await? {
            continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab | KeyCode::Char('v') => {
                app_state.active_view = app_state.active_view.toggle();
                if app_state.active_view == TuiView::NetWorth && !app_state.net_worth_loaded {
                    refresh_net_worth(app_state, storage.clone(), config).await;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let active_table_state = active_table_state_mut(
                    app_state.active_view,
                    tx_table_state,
                    net_worth_table_state,
                );
                select_prev(app_state.visible_row_count(), active_table_state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let active_table_state = active_table_state_mut(
                    app_state.active_view,
                    tx_table_state,
                    net_worth_table_state,
                );
                select_next(app_state.visible_row_count(), active_table_state);
            }
            KeyCode::Char('s') if app_state.active_view == TuiView::Transactions => {
                app_state.sort = app_state.sort.next();
                app_state.recompute_visible_transactions();
            }
            KeyCode::Char('1') => {
                app_state.span = TimeSpan::Days7;
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('2') => {
                app_state.span = TimeSpan::Days30;
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('3') => {
                app_state.span = TimeSpan::Days90;
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('4') => {
                app_state.span = TimeSpan::Days365;
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('5') => {
                app_state.span = TimeSpan::All;
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('[') => {
                app_state.span = app_state.span.prev();
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char(']') => {
                app_state.span = app_state.span.next();
                app_state.recompute_visible_transactions();
                app_state.recompute_visible_net_worth();
            }
            KeyCode::Char('-') if app_state.active_view == TuiView::NetWorth => {
                app_state.net_worth_interval = app_state.net_worth_interval.prev();
                refresh_net_worth(app_state, storage.clone(), config).await;
            }
            KeyCode::Char('=') | KeyCode::Char('+')
                if app_state.active_view == TuiView::NetWorth =>
            {
                app_state.net_worth_interval = app_state.net_worth_interval.next();
                refresh_net_worth(app_state, storage.clone(), config).await;
            }
            KeyCode::Char('r') => match app_state.active_view {
                TuiView::Transactions => {
                    refresh_transactions_and_rules(app_state, storage.as_ref(), config).await?;
                    app_state.status_message =
                        Some("Reloaded transactions and tag rules".to_string());
                }
                TuiView::NetWorth => {
                    refresh_net_worth(app_state, storage.clone(), config).await;
                }
            },
            KeyCode::Char('i') if app_state.active_view == TuiView::Transactions => {
                app_state.include_ignored = !app_state.include_ignored;
                refresh_transactions_and_rules(app_state, storage.as_ref(), config).await?;
                app_state.status_message = Some(format!(
                    "include_ignored={}",
                    if app_state.include_ignored {
                        "yes"
                    } else {
                        "no"
                    }
                ));
            }
            KeyCode::Char('c') if app_state.active_view == TuiView::Transactions => {
                open_tag_modal_for_selected(app_state, tx_table_state, TagActionKind::OneOff);
            }
            KeyCode::Char('C') if app_state.active_view == TuiView::Transactions => {
                open_tag_modal_for_selected(app_state, tx_table_state, TagActionKind::Rule);
            }
            _ => {}
        }
    }
}

fn render(
    frame: &mut Frame<'_>,
    app_state: &AppState,
    tx_table_state: &mut TableState,
    net_worth_table_state: &mut TableState,
    config: &ResolvedConfig,
) {
    let summary_height = match app_state.active_view {
        TuiView::Transactions => 4,
        TuiView::NetWorth => 3,
    };
    let [summary_area, table_area, help_area] = Layout::vertical([
        Constraint::Length(summary_height),
        Constraint::Min(5),
        Constraint::Length(4),
    ])
    .areas(frame.area());

    match app_state.active_view {
        TuiView::Transactions => render_transactions_view(
            frame,
            app_state,
            tx_table_state,
            summary_area,
            table_area,
            help_area,
            config,
        ),
        TuiView::NetWorth => render_net_worth_view(
            frame,
            app_state,
            net_worth_table_state,
            summary_area,
            table_area,
            help_area,
        ),
    }

    if let Some(modal) = app_state.modal.as_ref() {
        render_modal(frame, modal);
    }
}

fn centered_rect(
    width_percent: u16,
    height_percent: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let [vertical] = Layout::vertical([Constraint::Percentage(height_percent)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Percentage(width_percent)])
        .flex(ratatui::layout::Flex::Center)
        .areas(vertical);
    horizontal
}

fn render_modal(frame: &mut Frame<'_>, modal: &ModalState) {
    match modal {
        ModalState::Tag(tag_modal) => render_tag_modal(frame, tag_modal),
        ModalState::Regex(regex_modal) => render_regex_modal(frame, regex_modal),
    }
}

fn render_input_line(label: &str, input: &str, cursor: usize) -> Line<'static> {
    let cursor = clamp_cursor(input, cursor);
    let before: String = input.chars().take(cursor).collect();
    let after: String = input.chars().skip(cursor).collect();
    let mut spans = Vec::new();
    spans.push(Span::raw(format!("{label}: ")));
    spans.push(Span::raw(before));
    spans.push(Span::styled(
        "|",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(after));
    Line::from(spans)
}

fn render_tag_modal(frame: &mut Frame<'_>, modal: &TagModalState) {
    let popup = centered_rect(80, 62, frame.area());
    frame.render_widget(Clear, popup);

    let title = match &modal.action {
        TagAction::OneOff { .. } => "Set Tag",
        TagAction::Rule { .. } => "Create Tag Rule",
    };
    let source = match &modal.action {
        TagAction::OneOff { source } => source,
        TagAction::Rule { source } => source,
    };

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(format!(
            "tx={}  account={}  status={}",
            source.transaction_id, source.account_name, source.status
        )),
        Line::from(format!("description: {}", source.description)),
        Line::from(""),
        render_input_line("tag", &modal.input, modal.cursor),
        Line::from("type to filter | left/right/home/end move | backspace/delete edit"),
        Line::from("up/down select | tab autocomplete | enter confirm | esc cancel"),
        Line::from(""),
        Line::from("suggestions:"),
    ];

    if modal.suggestions.is_empty() {
        lines.push(Line::from("  (no matches)"));
    } else {
        for (index, suggestion) in modal.suggestions.iter().take(8).enumerate() {
            let is_selected = index == modal.selected_suggestion && modal.selection_active;
            let prefix = if is_selected { "> " } else { "  " };
            let span = if is_selected {
                Span::styled(
                    format!("{prefix}{suggestion}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(format!("{prefix}{suggestion}"))
            };
            lines.push(Line::from(span));
        }
    }

    let paragraph =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, popup);
}

fn render_regex_modal(frame: &mut Frame<'_>, modal: &RegexModalState) {
    let popup = centered_rect(80, 48, frame.area());
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from(format!(
            "tx={}  account={}",
            modal.source.transaction_id, modal.source.account_name
        )),
        Line::from(format!("tag: {}", modal.tag)),
        Line::from(""),
        render_input_line("regex", &modal.input, modal.cursor),
        Line::from("left/right/home/end move | backspace/delete edit"),
        Line::from("enter save rule | esc cancel"),
        Line::from(format!(
            "suggestion source: {}",
            if modal.used_llm_suggestion {
                "llm"
            } else {
                "fallback"
            }
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Edit Rule Regex"),
    );
    frame.render_widget(paragraph, popup);
}

fn render_transactions_view(
    frame: &mut Frame<'_>,
    app_state: &AppState,
    table_state: &mut TableState,
    summary_area: ratatui::layout::Rect,
    table_area: ratatui::layout::Rect,
    help_area: ratatui::layout::Rect,
    config: &ResolvedConfig,
) {
    let status_text = app_state.status_message.as_deref().unwrap_or("-");
    let spending_line = transaction_spending_summary_line(app_state, config);
    let summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "Transactions TUI  ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "span={} | sort={} | rows={} | total={} | include_ignored={} | refresh={} | status={}",
                app_state.span.label(),
                app_state.sort.label(),
                app_state.visible_transaction_indices.len(),
                app_state.all_transactions.len(),
                if app_state.include_ignored {
                    "yes"
                } else {
                    "no"
                },
                app_state
                    .transaction_last_refresh_utc
                    .format("%Y-%m-%d %H:%M:%S UTC"),
                status_text
            )),
        ]),
        Line::from(vec![
            Span::styled(
                "Spending  ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(spending_line),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("View"));
    frame.render_widget(summary, summary_area);

    let rows = app_state
        .visible_transaction_indices
        .iter()
        .map(|idx| &app_state.all_transactions[*idx])
        .map(|tx| {
            let amount_is_negative = Decimal::from_str(&tx.amount)
                .map(|v| v < Decimal::ZERO)
                .unwrap_or(false);
            let amount_style = if amount_is_negative {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            let amount = transaction_amount_string(tx, config);
            let description = tx
                .annotation
                .as_ref()
                .and_then(|ann| ann.description.as_deref())
                .unwrap_or(tx.description.as_str());
            Row::new(vec![
                Cell::from(transaction_date_string(tx)),
                Cell::from(tx.account_name.clone()),
                Cell::from(description.to_string()),
                Cell::from(transaction_tag_string(tx, &app_state.tag_matcher)),
                Cell::from(amount).style(amount_style),
                Cell::from(asset_label(&tx.asset)),
                Cell::from(tx.status.clone()),
            ])
        });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(22),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new([
            "date",
            "account",
            "description",
            "tag",
            "amount",
            "asset",
            "status",
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Transaction Log"),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol(">> ");
    frame.render_stateful_widget(table, table_area, table_state);

    let help = Paragraph::new(
        "q/esc quit | tab/v switch view | j/k or arrows move | 1..5 span | [ ] cycle span | s sort | i ignored | r reload | c one-off tag | C add regex rule",
    )
    .block(Block::default().borders(Borders::ALL).title("Keys"));
    frame.render_widget(help, help_area);
}

fn render_net_worth_view(
    frame: &mut Frame<'_>,
    app_state: &AppState,
    table_state: &mut TableState,
    summary_area: ratatui::layout::Rect,
    table_area: ratatui::layout::Rect,
    help_area: ratatui::layout::Rect,
) {
    let summary = Paragraph::new(Line::from(vec![
        Span::styled(
            "Net Worth TUI  ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "span={} | interval={} | rows={} | total={} | refresh={}",
            app_state.span.label(),
            app_state.net_worth_interval.label(),
            app_state.visible_net_worth_indices.len(),
            app_state.net_worth_points.len(),
            app_state
                .net_worth_last_refresh_utc
                .format("%Y-%m-%d %H:%M:%S UTC")
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("View"));
    frame.render_widget(summary, summary_area);

    if let Some(error) = app_state.net_worth_error.as_deref() {
        let paragraph = Paragraph::new(error.to_string())
            .block(Block::default().borders(Borders::ALL).title("Net Worth"));
        frame.render_widget(paragraph, table_area);
    } else {
        let rows = app_state
            .visible_net_worth_indices
            .iter()
            .map(|idx| &app_state.net_worth_points[*idx])
            .map(|point| {
                let delta_style = net_worth_delta_style(&point.percentage_change_from_previous);
                Row::new(vec![
                    Cell::from(point.date.clone()),
                    Cell::from(net_worth_time_string(point)),
                    Cell::from(point.total_value.clone()),
                    Cell::from(
                        point
                            .percentage_change_from_previous
                            .clone()
                            .unwrap_or_else(|| "-".to_string()),
                    )
                    .style(delta_style),
                    Cell::from(net_worth_trigger_count(point)),
                ])
            });

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(10),
            ],
        )
        .header(
            Row::new(["date", "time", "net_worth", "delta_%", "triggers"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Net Worth History"),
        )
        .row_highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol(">> ");
        frame.render_stateful_widget(table, table_area, table_state);
    }

    let help = Paragraph::new(
        "q/esc quit | tab/v switch view | j/k or arrows move | 1..5 span | [ ] cycle span | -/+ interval | r reload",
    )
    .block(Block::default().borders(Borders::ALL).title("Keys"));
    frame.render_widget(help, help_area);
}

async fn load_transactions(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    include_ignored: bool,
) -> Result<Vec<TransactionOutput>> {
    app::list_transactions(
        storage,
        Some(LOAD_START_DATE.to_string()),
        Some(LOAD_END_DATE.to_string()),
        None,
        false,
        !include_ignored,
        config,
    )
    .await
}

async fn load_net_worth(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    interval: NetWorthInterval,
) -> Result<Vec<HistoryPoint>> {
    let output = app::portfolio_history(
        storage,
        config,
        None,
        None,
        None,
        interval.as_granularity().to_string(),
        true,
        false,
    )
    .await?;
    Ok(output.points)
}

async fn refresh_net_worth(
    app_state: &mut AppState,
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
) {
    let refreshed_at = Utc::now();
    let result = load_net_worth(storage, config, app_state.net_worth_interval).await;
    app_state.net_worth_loaded = true;
    app_state.net_worth_last_refresh_utc = refreshed_at;

    match result {
        Ok(points) => {
            app_state.net_worth_points = points;
            app_state.net_worth_error = None;
            app_state.recompute_visible_net_worth();
        }
        Err(error) => {
            app_state.net_worth_points.clear();
            app_state.visible_net_worth_indices.clear();
            app_state.net_worth_error = Some(error.to_string());
        }
    }
}

fn compare_transactions(
    left: &TransactionOutput,
    right: &TransactionOutput,
    sort: SortMode,
) -> Ordering {
    match sort {
        SortMode::DateDesc => timestamp_order(left, right).reverse(),
        SortMode::DateAsc => timestamp_order(left, right),
        SortMode::AmountAsc => amount_order(left, right),
        SortMode::AmountDesc => amount_order(left, right).reverse(),
    }
}

fn timestamp_order(left: &TransactionOutput, right: &TransactionOutput) -> Ordering {
    let l = timestamp_sort_key(left);
    let r = timestamp_sort_key(right);
    l.cmp(&r)
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| left.account_id.cmp(&right.account_id))
}

fn amount_order(left: &TransactionOutput, right: &TransactionOutput) -> Ordering {
    let left_amount = Decimal::from_str(&left.amount);
    let right_amount = Decimal::from_str(&right.amount);
    match (left_amount, right_amount) {
        (Ok(l), Ok(r)) => l
            .cmp(&r)
            .then_with(|| timestamp_order(left, right).reverse()),
        (Err(_), Ok(_)) => Ordering::Greater,
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Err(_)) => left.amount.cmp(&right.amount),
    }
}

fn timestamp_sort_key(tx: &TransactionOutput) -> i64 {
    DateTime::parse_from_rfc3339(&tx.timestamp)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MIN)
}

fn transaction_date(tx: &TransactionOutput) -> Option<NaiveDate> {
    tx.timestamp
        .get(..10)
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

fn net_worth_point_date(point: &HistoryPoint) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            point
                .timestamp
                .get(..10)
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        })
}

fn net_worth_timestamp_sort_key(point: &HistoryPoint) -> i64 {
    DateTime::parse_from_rfc3339(&point.timestamp)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MIN)
}

fn transaction_date_string(tx: &TransactionOutput) -> String {
    tx.timestamp
        .get(..10)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| tx.timestamp.clone())
}

fn resolved_transaction_tag(
    tx: &TransactionOutput,
    matcher: &TransactionTagMatcher,
) -> Option<String> {
    let annotation_tag = tx
        .annotation
        .as_ref()
        .and_then(|ann| ann.tags.as_ref())
        .and_then(|tags| tags.first())
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if annotation_tag.is_some() {
        return annotation_tag;
    }

    let effective_tag = tx
        .tags
        .first()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_subtag = tx
        .subtags
        .first()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rule_tag = matcher
        .match_tag(&TransactionTagRuleInput {
            account_id: &tx.account_id,
            account_name: &tx.account_name,
            description: &tx.description,
            tag: effective_tag.unwrap_or(""),
            subtag: effective_subtag.unwrap_or(""),
            status: &tx.status,
            amount: &tx.amount,
        })
        .map(ToOwned::to_owned);
    if rule_tag.is_some() {
        return rule_tag;
    }

    effective_tag.map(ToOwned::to_owned)
}

fn transaction_tag_string(tx: &TransactionOutput, matcher: &TransactionTagMatcher) -> String {
    resolved_transaction_tag(tx, matcher).unwrap_or_else(|| "-".to_string())
}

fn transaction_amount_string(tx: &TransactionOutput, config: &ResolvedConfig) -> String {
    let Ok(amount) = Decimal::from_str(&tx.amount) else {
        return tx.amount.clone();
    };
    if transaction_asset_is_reporting_currency(tx, &config.reporting_currency) {
        format_base_currency_display(
            amount,
            config.display.currency_decimals,
            config.display.currency_grouping,
            config
                .display
                .currency_symbol
                .as_deref()
                .or_else(|| currency_symbol(&config.reporting_currency)),
            config.display.currency_fixed_decimals,
        )
    } else {
        amount.normalize().to_string()
    }
}

fn transaction_asset_is_reporting_currency(
    tx: &TransactionOutput,
    reporting_currency: &str,
) -> bool {
    let Some(obj) = tx.asset.as_object() else {
        return false;
    };
    let is_currency = obj.get("type").and_then(|v| v.as_str()) == Some("currency");
    if !is_currency {
        return false;
    }
    let Some(iso_code) = obj.get("iso_code").and_then(|v| v.as_str()) else {
        return false;
    };
    normalize_currency_code_for_display(iso_code) == reporting_currency.trim().to_uppercase()
}

#[derive(Debug, Clone)]
struct SpendingWindowSummary {
    days: u32,
    total: Decimal,
    transaction_count: usize,
}

fn spending_windows_from_config(config: &ResolvedConfig) -> Vec<u32> {
    let mut windows: Vec<u32> = config
        .tray
        .spending_windows_days
        .iter()
        .copied()
        .filter(|days| *days > 0)
        .collect();
    if windows.is_empty() {
        windows.extend([7, 30, 90]);
    }
    windows.sort_unstable();
    windows.dedup();
    windows
}

fn summarize_spending_windows(
    transactions: &[TransactionOutput],
    reporting_currency: &str,
    windows_days: &[u32],
    today: NaiveDate,
) -> Vec<SpendingWindowSummary> {
    let mut summaries: Vec<SpendingWindowSummary> = windows_days
        .iter()
        .copied()
        .map(|days| SpendingWindowSummary {
            days,
            total: Decimal::ZERO,
            transaction_count: 0,
        })
        .collect();

    if summaries.is_empty() {
        return summaries;
    }

    for tx in transactions {
        if transaction_annotation_ignores_spending(tx.annotation.as_ref()) {
            continue;
        }
        if !transaction_asset_is_reporting_currency(tx, reporting_currency) {
            continue;
        }
        let Some(date) = transaction_date(tx) else {
            continue;
        };
        let age_days = (today - date).num_days();
        if age_days < 0 {
            continue;
        }
        let Ok(amount) = Decimal::from_str(&tx.amount) else {
            continue;
        };
        if amount >= Decimal::ZERO {
            continue;
        }

        let spend_amount = -amount;
        for summary in &mut summaries {
            if age_days <= summary.days as i64 {
                summary.total += spend_amount;
                summary.transaction_count += 1;
            }
        }
    }

    summaries
}

fn transaction_annotation_ignores_spending(
    annotation: Option<&crate::app::TransactionAnnotationOutput>,
) -> bool {
    annotation.is_some_and(|ann| {
        ann.ignore_spending == Some(true)
            || ann.tags.as_ref().is_some_and(|tags| {
                tags.iter()
                    .any(|tag| crate::models::tag_ignores_spending(tag))
            })
    })
}

fn transaction_spending_summary_line(app_state: &AppState, config: &ResolvedConfig) -> String {
    let windows = spending_windows_from_config(config);
    let summaries = summarize_spending_windows(
        &app_state.all_transactions,
        &config.reporting_currency,
        &windows,
        Utc::now().date_naive(),
    );
    if summaries.is_empty() {
        return "no windows configured".to_string();
    }

    let max_windows = 4usize;
    let shown = summaries.len().min(max_windows);
    let mut parts: Vec<String> = Vec::with_capacity(shown + 1);
    for summary in summaries.iter().take(shown) {
        let total = format_base_currency_display(
            summary.total,
            config.display.currency_decimals,
            config.display.currency_grouping,
            config
                .display
                .currency_symbol
                .as_deref()
                .or_else(|| currency_symbol(&config.reporting_currency)),
            config.display.currency_fixed_decimals,
        );
        parts.push(format!(
            "{}d: {} ({} txns)",
            summary.days, total, summary.transaction_count
        ));
    }
    if summaries.len() > shown {
        parts.push(format!("+{} more", summaries.len() - shown));
    }
    parts.join(" | ")
}

fn net_worth_time_string(point: &HistoryPoint) -> String {
    point
        .timestamp
        .get(11..19)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| point.timestamp.clone())
}

fn net_worth_trigger_count(point: &HistoryPoint) -> String {
    point
        .change_triggers
        .as_ref()
        .map(|triggers| triggers.len().to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn net_worth_delta_style(delta: &Option<String>) -> Style {
    let Some(delta_value) = delta.as_ref() else {
        return Style::default();
    };

    let parsed = Decimal::from_str(delta_value);
    match parsed {
        Ok(value) if value < Decimal::ZERO => Style::default().fg(Color::Red),
        Ok(value) if value > Decimal::ZERO => Style::default().fg(Color::Green),
        _ => Style::default(),
    }
}

fn asset_label(asset: &serde_json::Value) -> String {
    let Some(obj) = asset.as_object() else {
        return asset.to_string();
    };
    let Some(kind) = obj.get("type").and_then(|v| v.as_str()) else {
        return asset.to_string();
    };
    match kind {
        "currency" => obj
            .get("iso_code")
            .and_then(|v| v.as_str())
            .map(normalize_currency_code_for_display)
            .unwrap_or_else(|| "currency".to_string()),
        "equity" => obj
            .get("symbol")
            .and_then(|v| v.as_str())
            .map(|s| format!("equity:{s}"))
            .unwrap_or_else(|| "equity".to_string()),
        "crypto" => {
            let symbol = obj.get("symbol").and_then(|v| v.as_str()).unwrap_or("?");
            if let Some(network) = obj.get("network").and_then(|v| v.as_str()) {
                format!("crypto:{network}:{symbol}")
            } else {
                format!("crypto:{symbol}")
            }
        }
        _ => asset.to_string(),
    }
}

fn normalize_currency_code_for_display(value: &str) -> String {
    let trimmed = value.trim();
    match trimmed {
        "840" => "USD".to_string(),
        _ => trimmed.to_uppercase(),
    }
}

fn clamp_selection(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let selected = table_state.selected().unwrap_or(0);
    let clamped = selected.min(visible_len.saturating_sub(1));
    table_state.select(Some(clamped));
}

fn select_prev(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let next = table_state.selected().unwrap_or(0).saturating_sub(1);
    table_state.select(Some(next));
}

fn select_next(visible_len: usize, table_state: &mut TableState) {
    if visible_len == 0 {
        table_state.select(None);
        return;
    }
    let current = table_state.selected().unwrap_or(0);
    let max_index = visible_len.saturating_sub(1);
    table_state.select(Some((current + 1).min(max_index)));
}

fn active_table_state_mut<'a>(
    active_view: TuiView,
    tx_table_state: &'a mut TableState,
    net_worth_table_state: &'a mut TableState,
) -> &'a mut TableState {
    match active_view {
        TuiView::Transactions => tx_table_state,
        TuiView::NetWorth => net_worth_table_state,
    }
}

fn enter_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/tui/mod_tests.rs"]
mod mod_tests;
