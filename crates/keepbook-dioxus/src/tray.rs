use crate::{Overview, TraySnapshot};
use dioxus::desktop::trayicon::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu},
    Icon, TrayIconBuilder,
};
use dioxus::desktop::{use_tray_menu_event_handler, window};
use dioxus::prelude::*;
use image::ImageReader;
use std::io::Cursor;

const SHOW_HIDE_ID: &str = "keepbook-show-hide";
const OPEN_APP_ID: &str = "keepbook-open-app";
const SHOW_SETTINGS_ID: &str = "keepbook-show-settings";
const SYNC_NOW_ID: &str = "keepbook-sync-now";
const QUIT_ID: &str = "keepbook-quit";

#[derive(Clone)]
struct TrayState {
    tray: dioxus::desktop::trayicon::TrayIcon,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrayRuntime {
    pub status_text: String,
    pub last_cycle_text: String,
    pub next_cycle_text: String,
    pub last_summary: String,
}

impl Default for TrayRuntime {
    fn default() -> Self {
        Self {
            status_text: "Idle".to_string(),
            last_cycle_text: "Last price refresh: never".to_string(),
            next_cycle_text: "Next price refresh: unscheduled".to_string(),
            last_summary: "No price refresh has run yet".to_string(),
        }
    }
}

pub fn show_window() {
    let win = window();
    win.set_visible(true);
    win.set_focus();
}

fn toggle_window_visibility() {
    let win = window();
    if win.is_visible() {
        win.set_visible(false);
    } else {
        win.set_visible(true);
        win.set_focus();
    }
}

#[component]
pub fn KeepbookTray(
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    runtime: TrayRuntime,
    onsyncnow: EventHandler<()>,
) -> Element {
    let tray_state = use_hook(create_tray_state);

    use_tray_menu_event_handler(move |event| match event.id().as_ref() {
        OPEN_APP_ID => show_window(),
        SHOW_HIDE_ID => toggle_window_visibility(),
        SYNC_NOW_ID => {
            show_window();
            onsyncnow.call(());
        }
        QUIT_ID => std::process::exit(0),
        _ => {}
    });

    use_effect(use_reactive!(|overview, tray_snapshot, runtime| {
        if let Some(tray_state) = tray_state.as_ref() {
            update_tray_state(
                tray_state,
                overview.as_ref(),
                tray_snapshot.as_ref(),
                &runtime,
            );
        }
    }));

    rsx! {}
}

#[component]
pub fn TrayViewActions(onshowsettings: EventHandler<()>) -> Element {
    use_tray_menu_event_handler(move |event| match event.id().as_ref() {
        SHOW_SETTINGS_ID => {
            show_window();
            onshowsettings.call(());
        }
        _ => {}
    });

    rsx! {}
}

fn create_tray_state() -> Option<TrayState> {
    match create_tray_state_inner() {
        Ok(state) => Some(state),
        Err(error) => {
            eprintln!("Failed to initialize keepbook tray icon: {error}");
            None
        }
    }
}

fn create_tray_state_inner() -> Result<TrayState, String> {
    let menu = build_menu(None, None, &TrayRuntime::default())?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("Keepbook")
        .with_icon(load_tray_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("failed to build keepbook tray icon: {error}"))?;

    Ok(TrayState { tray })
}

fn update_tray_state(
    state: &TrayState,
    overview: Option<&Overview>,
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
    runtime: &TrayRuntime,
) {
    match build_menu(overview, tray_snapshot, runtime) {
        Ok(menu) => state.tray.set_menu(Some(Box::new(menu))),
        Err(error) => eprintln!("Failed to update keepbook tray menu: {error}"),
    }

    let tooltip = tray_tooltip(tray_snapshot, runtime);
    let _ = state.tray.set_tooltip(Some(tooltip));

    if let Some(Ok(snapshot)) = tray_snapshot {
        state
            .tray
            .set_title(Some(short_title(&snapshot.total_label)));
    } else {
        state.tray.set_title(Some("Keepbook"));
    }
}

fn build_menu(
    overview: Option<&Overview>,
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
    runtime: &TrayRuntime,
) -> Result<Menu, String> {
    let menu = Menu::new();

    append_disabled(&menu, "keepbook")?;
    append_separator(&menu)?;
    append_disabled(&menu, format!("Status: {}", runtime.status_text))?;
    append_disabled(&menu, &runtime.last_cycle_text)?;
    append_disabled(&menu, &runtime.next_cycle_text)?;
    append_disabled(&menu, &runtime.last_summary)?;
    append_separator(&menu)?;

    let (history_lines, breakdown_lines, spending_lines, transaction_lines) =
        tray_lines(tray_snapshot, overview);

    append_submenu(&menu, "Recent Portfolio History", &history_lines)?;
    append_submenu(&menu, "Portfolio Breakdown", &breakdown_lines)?;
    append_disabled(&menu, "Recent Spending")?;
    append_disabled_lines(&menu, &spending_lines)?;
    append_submenu(&menu, "Recent Transactions", &transaction_lines)?;
    append_separator(&menu)?;

    append_action(&menu, SYNC_NOW_ID, "Refresh Prices")?;
    append_action(&menu, OPEN_APP_ID, "Open App")?;
    append_action(&menu, SHOW_SETTINGS_ID, "Settings")?;
    append_action(&menu, SHOW_HIDE_ID, "Show/Hide Window")?;
    append_separator(&menu)?;
    append_action(&menu, QUIT_ID, "Quit")?;

    Ok(menu)
}

fn tray_lines(
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
    overview: Option<&Overview>,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    match tray_snapshot {
        Some(Ok(snapshot)) => (
            fallback_line(&snapshot.history_lines, "No portfolio history available"),
            fallback_line(
                &snapshot.portfolio_breakdown_lines,
                "No portfolio breakdown available",
            ),
            fallback_line(&snapshot.spending_lines, "No spending metrics available"),
            fallback_line(&snapshot.transaction_lines, "No recent transactions"),
        ),
        Some(Err(error)) => {
            let breakdown = overview
                .map(overview_breakdown_lines)
                .unwrap_or_else(|| vec!["No portfolio breakdown available".to_string()]);
            (
                vec![format!("History unavailable: {error}")],
                breakdown,
                vec![format!("Spending unavailable: {error}")],
                vec![format!("Transactions unavailable: {error}")],
            )
        }
        None => {
            let breakdown = overview
                .map(overview_breakdown_lines)
                .unwrap_or_else(|| vec!["Portfolio breakdown loading".to_string()]);
            (
                vec!["Portfolio history loading".to_string()],
                breakdown,
                vec!["Spending metrics loading".to_string()],
                vec!["Transactions loading".to_string()],
            )
        }
    }
}

fn overview_breakdown_lines(overview: &Overview) -> Vec<String> {
    let mut lines = vec![format!(
        "Total: {}",
        overview
            .snapshot
            .total_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| crate::logic::format_full_money(value, &overview.snapshot.currency))
            .unwrap_or_else(|| overview.snapshot.total_value.clone())
    )];

    if overview.snapshot.by_account.is_empty() {
        lines.push("No accounts with balances".to_string());
        return lines;
    }

    lines.extend(overview.snapshot.by_account.iter().map(|account| {
        let value = account
            .value_in_base
            .as_deref()
            .and_then(|raw| raw.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map(|value| crate::logic::format_full_money(value, &overview.snapshot.currency))
            .unwrap_or_else(|| "unpriced".to_string());
        format!(
            "{} / {}: {}",
            account.connection_name, account.account_name, value
        )
    }));

    lines
}

fn fallback_line(lines: &[String], fallback: &str) -> Vec<String> {
    if lines.is_empty() {
        vec![fallback.to_string()]
    } else {
        lines.to_vec()
    }
}

fn append_disabled(menu: &Menu, label: impl AsRef<str>) -> Result<(), String> {
    let item = MenuItem::new(label.as_ref(), false, None);
    menu.append(&item)
        .map_err(|error| format!("failed to append tray row: {error}"))
}

fn append_disabled_lines(menu: &Menu, lines: &[String]) -> Result<(), String> {
    for line in lines {
        append_disabled(menu, line)?;
    }
    Ok(())
}

fn append_action(menu: &Menu, id: &str, label: &str) -> Result<(), String> {
    let item = MenuItem::with_id(MenuId::new(id), label, true, None);
    menu.append(&item)
        .map_err(|error| format!("failed to append tray action: {error}"))
}

fn append_submenu(menu: &Menu, label: &str, lines: &[String]) -> Result<(), String> {
    let submenu = Submenu::new(label, true);
    for line in lines {
        let item = MenuItem::new(line, false, None);
        submenu
            .append(&item)
            .map_err(|error| format!("failed to append tray submenu row: {error}"))?;
    }
    menu.append(&submenu)
        .map_err(|error| format!("failed to append tray submenu: {error}"))
}

fn append_separator(menu: &Menu) -> Result<(), String> {
    let separator = PredefinedMenuItem::separator();
    menu.append(&separator)
        .map_err(|error| format!("failed to append tray separator: {error}"))
}

fn tray_tooltip(
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
    runtime: &TrayRuntime,
) -> String {
    let mut lines = vec![
        "Keepbook".to_string(),
        runtime.status_text.clone(),
        runtime.last_cycle_text.clone(),
        runtime.next_cycle_text.clone(),
        runtime.last_summary.clone(),
    ];

    match tray_snapshot {
        Some(Ok(snapshot)) => {
            lines.push(format!("Net worth: {}", snapshot.total_label));
            lines.push(format!("As of: {}", snapshot.as_of_date));
        }
        Some(Err(error)) => lines.push(format!("Tray data unavailable: {error}")),
        None => lines.push("Tray data loading".to_string()),
    }

    lines.join("\n")
}

fn short_title(label: &str) -> String {
    label
        .replace(['$', ','], "")
        .split('.')
        .next()
        .unwrap_or(label)
        .to_string()
}

fn load_tray_icon() -> Result<Icon, String> {
    let bytes = include_bytes!("../../../assets/keepbook-icon-64.png");
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("failed to guess keepbook tray icon format: {error}"))?
        .decode()
        .map_err(|error| format!("failed to decode keepbook tray icon: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height)
        .map_err(|error| format!("failed to load keepbook tray icon: {error}"))
}
