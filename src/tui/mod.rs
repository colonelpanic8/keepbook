mod data;
mod display;
mod input;
mod render;
mod state;

use std::io;
use std::sync::Arc;

use anyhow::Result;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::TableState;
use ratatui::Terminal;

use crate::app::transaction_tag_rules::{load_transaction_tag_rules, tag_rules_path};
use crate::config::ResolvedConfig;
use crate::storage::Storage;

pub use state::{NetWorthInterval, TuiOptions, TuiView};

use data::{load_transactions, refresh_net_worth};
use input::run_event_loop;
use state::AppState;

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
