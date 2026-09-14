//! Key handling: the event loop, the tag and regex modals, and text editing.

use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::TableState;
use ratatui::Terminal;
use regex::Regex;

use crate::app;
use crate::app::transaction_tag_rules::{
    append_transaction_tag_rule, exact_ci_regex_pattern, fallback_regex_suggestion,
    load_transaction_tag_rules, suggest_regex_with_openai, TransactionTagRule,
};
use crate::config::ResolvedConfig;
use crate::storage::Storage;

use super::data::{refresh_net_worth, refresh_transactions_and_rules};
use super::display::resolved_transaction_tag;
use super::render::render;
use super::state::{
    active_table_state_mut, clamp_selection, select_next, select_prev, select_transaction_by_id,
    selected_transaction, AppState, ModalState, RegexModalState, SelectedTransactionInfo,
    TagAction, TagModalState, TimeSpan, TuiView,
};

const POLL_INTERVAL: Duration = Duration::from_millis(200);

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

pub(super) fn clamp_cursor(input: &str, cursor: usize) -> usize {
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

pub(super) async fn run_event_loop(
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

#[cfg(test)]
#[path = "../../tests/unit/tui/input_tests.rs"]
mod input_tests;
