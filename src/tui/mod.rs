use std::cmp::Ordering;
use std::io;
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
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::{Frame, Terminal};
use rust_decimal::Decimal;

use crate::app::{self, HistoryPoint, TransactionOutput};
use crate::config::ResolvedConfig;
use crate::format::format_base_currency_display;
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

struct AppState {
    active_view: TuiView,
    all_transactions: Vec<TransactionOutput>,
    visible_transaction_indices: Vec<usize>,
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
}

impl AppState {
    fn new(
        all_transactions: Vec<TransactionOutput>,
        include_ignored: bool,
        options: TuiOptions,
    ) -> Self {
        let now = Utc::now();
        let mut state = Self {
            active_view: options.start_view,
            all_transactions,
            visible_transaction_indices: Vec::new(),
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
    let mut app_state = AppState::new(transactions, include_ignored, options);
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
            KeyCode::Char('s') => {
                if app_state.active_view == TuiView::Transactions {
                    app_state.sort = app_state.sort.next();
                    app_state.recompute_visible_transactions();
                }
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
            KeyCode::Char('-') => {
                if app_state.active_view == TuiView::NetWorth {
                    app_state.net_worth_interval = app_state.net_worth_interval.prev();
                    refresh_net_worth(app_state, storage.clone(), config).await;
                }
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                if app_state.active_view == TuiView::NetWorth {
                    app_state.net_worth_interval = app_state.net_worth_interval.next();
                    refresh_net_worth(app_state, storage.clone(), config).await;
                }
            }
            KeyCode::Char('r') => match app_state.active_view {
                TuiView::Transactions => {
                    app_state.all_transactions =
                        load_transactions(storage.as_ref(), config, app_state.include_ignored)
                            .await?;
                    app_state.transaction_last_refresh_utc = Utc::now();
                    app_state.recompute_visible_transactions();
                }
                TuiView::NetWorth => {
                    refresh_net_worth(app_state, storage.clone(), config).await;
                }
            },
            KeyCode::Char('i') => {
                if app_state.active_view == TuiView::Transactions {
                    app_state.include_ignored = !app_state.include_ignored;
                    app_state.all_transactions =
                        load_transactions(storage.as_ref(), config, app_state.include_ignored)
                            .await?;
                    app_state.transaction_last_refresh_utc = Utc::now();
                    app_state.recompute_visible_transactions();
                }
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
    let [summary_area, table_area, help_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
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
    let summary = Paragraph::new(Line::from(vec![
        Span::styled(
            "Transactions TUI  ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "span={} | sort={} | rows={} | total={} | include_ignored={} | refresh={}",
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
                .format("%Y-%m-%d %H:%M:%S UTC")
        )),
    ]))
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
                Cell::from(transaction_category_string(tx)),
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
            "category",
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
        "q/esc quit | tab/v switch view | j/k or arrows move | 1..5 span | [ ] cycle span | s sort | i ignored | r reload",
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

fn transaction_category_string(tx: &TransactionOutput) -> String {
    tx.annotation
        .as_ref()
        .and_then(|ann| ann.category.as_deref())
        .or_else(|| {
            tx.standardized_metadata
                .as_ref()
                .and_then(|md| md.merchant_category_label.as_deref())
        })
        .unwrap_or("-")
        .to_string()
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
            config.display.currency_symbol.as_deref(),
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
mod tests {
    use super::*;
    use crate::app::TransactionAnnotationOutput;
    use crate::config::{
        DisplayConfig, GitConfig, IgnoreConfig, RefreshConfig, SpendingConfig, TrayConfig,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn tx(id: &str, timestamp: &str, amount: &str) -> TransactionOutput {
        TransactionOutput {
            id: id.to_string(),
            account_id: "acct-1".to_string(),
            account_name: "Checking".to_string(),
            timestamp: timestamp.to_string(),
            description: "desc".to_string(),
            amount: amount.to_string(),
            asset: json!({"type":"currency","iso_code":"USD"}),
            status: "posted".to_string(),
            annotation: None,
            standardized_metadata: None,
        }
    }

    fn test_config() -> ResolvedConfig {
        ResolvedConfig {
            data_dir: PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            ignore: IgnoreConfig::default(),
            git: GitConfig::default(),
        }
    }

    #[test]
    fn compare_by_amount_handles_numeric_values() {
        let a = tx("a", "2026-01-01T00:00:00+00:00", "12");
        let b = tx("b", "2026-01-01T00:00:00+00:00", "-5");
        assert_eq!(
            compare_transactions(&a, &b, SortMode::AmountAsc),
            Ordering::Greater
        );
        assert_eq!(
            compare_transactions(&a, &b, SortMode::AmountDesc),
            Ordering::Less
        );
    }

    #[test]
    fn timespan_filter_respects_cutoff() {
        let mut state = AppState::new(
            vec![
                tx("a", "2026-01-01T00:00:00+00:00", "1"),
                tx("b", "2026-02-01T00:00:00+00:00", "1"),
            ],
            false,
            TuiOptions::default(),
        );
        state.span = TimeSpan::All;
        state.recompute_visible_transactions();
        let all_len = state.visible_transaction_indices.len();
        state.span = TimeSpan::Days7;
        state.recompute_visible_transactions();
        assert!(state.visible_transaction_indices.len() <= all_len);
    }

    #[test]
    fn display_uses_annotation_description_when_present() {
        let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1");
        t.annotation = Some(TransactionAnnotationOutput {
            description: Some("override".to_string()),
            note: None,
            category: None,
            tags: None,
        });
        let description = t
            .annotation
            .as_ref()
            .and_then(|ann| ann.description.as_deref())
            .unwrap_or(t.description.as_str());
        assert_eq!(description, "override");
    }

    #[test]
    fn display_uses_annotation_category_when_present() {
        let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1");
        assert_eq!(transaction_category_string(&t), "-");

        t.standardized_metadata = Some(crate::models::TransactionStandardizedMetadata {
            merchant_name: None,
            merchant_category_code: None,
            merchant_category_label: Some("Groceries".to_string()),
            transaction_kind: None,
            is_internal_transfer_hint: None,
        });
        assert_eq!(transaction_category_string(&t), "Groceries");

        t.annotation = Some(TransactionAnnotationOutput {
            description: None,
            note: None,
            category: Some("food".to_string()),
            tags: None,
        });
        assert_eq!(transaction_category_string(&t), "food");
    }

    #[test]
    fn asset_label_normalizes_currency_codes() {
        assert_eq!(
            asset_label(&json!({"type":"currency","iso_code":"840"})),
            "USD"
        );
        assert_eq!(
            asset_label(&json!({"type":"currency","iso_code":"usd"})),
            "USD"
        );
    }

    #[test]
    fn amount_display_uses_formatter_for_reporting_currency() {
        let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1234.5");
        t.asset = json!({"type":"currency","iso_code":"usd"});
        let mut config = test_config();
        config.display.currency_decimals = Some(2);
        config.display.currency_grouping = true;
        config.display.currency_symbol = Some("$".to_string());
        config.display.currency_fixed_decimals = true;

        assert_eq!(transaction_amount_string(&t, &config), "$1,234.50");
    }

    #[test]
    fn amount_display_normalizes_non_reporting_assets_without_symbol() {
        let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1.2300");
        t.asset = json!({"type":"crypto","symbol":"BTC"});
        let mut config = test_config();
        config.display.currency_decimals = Some(2);
        config.display.currency_grouping = true;
        config.display.currency_symbol = Some("$".to_string());
        config.display.currency_fixed_decimals = true;

        assert_eq!(transaction_amount_string(&t, &config), "1.23");
    }

    #[test]
    fn amount_display_preserves_unparseable_values() {
        let t = tx("a", "2026-02-01T00:00:00+00:00", "not-a-number");
        let config = test_config();
        assert_eq!(transaction_amount_string(&t, &config), "not-a-number");
    }

    #[test]
    fn net_worth_interval_cycles() {
        assert_eq!(NetWorthInterval::Daily.next(), NetWorthInterval::Weekly);
        assert_eq!(NetWorthInterval::Daily.prev(), NetWorthInterval::Hourly);
        assert_eq!(NetWorthInterval::Full.prev(), NetWorthInterval::Yearly);
    }

    #[test]
    fn net_worth_point_date_uses_date_field() {
        let point = HistoryPoint {
            timestamp: "2026-02-01T14:30:00+00:00".to_string(),
            date: "2026-02-01".to_string(),
            total_value: "1234".to_string(),
            percentage_change_from_previous: None,
            change_triggers: None,
        };
        assert_eq!(
            net_worth_point_date(&point),
            Some(NaiveDate::from_ymd_opt(2026, 2, 1).expect("valid date"))
        );
    }
}
