//! Ratatui widget construction for the two views and the modals.

use std::str::FromStr;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;

use super::display::{
    asset_label, net_worth_delta_style, net_worth_time_string, net_worth_trigger_count,
    transaction_amount_string, transaction_date_string, transaction_spending_summary_line,
    transaction_tag_string,
};
use super::input::clamp_cursor;
use super::state::{AppState, ModalState, RegexModalState, TagAction, TagModalState, TuiView};

pub(super) fn render(
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
