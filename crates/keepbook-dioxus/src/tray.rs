use crate::{
    logic::{current_net_worth_from_snapshot, format_full_money},
    AccountSummary, Overview,
};
use dioxus::desktop::trayicon::{
    menu::{Menu, MenuId, MenuItem, Submenu},
    Icon, TrayIconBuilder,
};
use dioxus::desktop::{use_tray_menu_event_handler, window};
use dioxus::prelude::*;
use image::ImageReader;
use std::io::Cursor;

const SHOW_HIDE_ID: &str = "keepbook-show-hide";
const SHOW_GRAPHS_ID: &str = "keepbook-show-graphs";
const SHOW_SETTINGS_ID: &str = "keepbook-show-settings";
const REFRESH_ID: &str = "keepbook-refresh";
const QUIT_ID: &str = "keepbook-quit";
const TOP_ACCOUNT_ROWS: usize = 5;

#[derive(Clone)]
struct TrayState {
    tray: dioxus::desktop::trayicon::TrayIcon,
    title_item: MenuItem,
    net_worth_item: MenuItem,
    accounts_item: MenuItem,
    connections_item: MenuItem,
    as_of_item: MenuItem,
    top_accounts: Vec<MenuItem>,
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
pub fn KeepbookTray(overview: Option<Overview>, onrefresh: EventHandler<()>) -> Element {
    let tray_state = use_hook(create_tray_state);

    use_tray_menu_event_handler(move |event| match event.id().as_ref() {
        SHOW_HIDE_ID => toggle_window_visibility(),
        REFRESH_ID => {
            show_window();
            onrefresh.call(());
        }
        QUIT_ID => std::process::exit(0),
        _ => {}
    });

    use_effect(move || {
        if let Some(tray_state) = tray_state.as_ref() {
            update_tray_state(tray_state, overview.as_ref());
        }
    });

    rsx! {}
}

#[component]
pub fn TrayViewActions(
    onshowgraphs: EventHandler<()>,
    onshowsettings: EventHandler<()>,
) -> Element {
    use_tray_menu_event_handler(move |event| match event.id().as_ref() {
        SHOW_GRAPHS_ID => {
            show_window();
            onshowgraphs.call(());
        }
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
    let title_item = MenuItem::new("Keepbook", false, None);
    let net_worth_item = MenuItem::new("Net worth: loading", false, None);
    let accounts_item = MenuItem::new("Accounts: loading", false, None);
    let connections_item = MenuItem::new("Connections: loading", false, None);
    let as_of_item = MenuItem::new("As of: loading", false, None);
    let show_hide_item =
        MenuItem::with_id(MenuId::new(SHOW_HIDE_ID), "Show/Hide Window", true, None);
    let graphs_item = MenuItem::with_id(MenuId::new(SHOW_GRAPHS_ID), "Open Graphs", true, None);
    let settings_item = MenuItem::with_id(MenuId::new(SHOW_SETTINGS_ID), "Settings", true, None);
    let refresh_item = MenuItem::with_id(MenuId::new(REFRESH_ID), "Refresh", true, None);
    let quit_item = MenuItem::with_id(MenuId::new(QUIT_ID), "Quit", true, None);

    let top_accounts_menu = Submenu::new("Top Accounts", true);
    let top_accounts: Vec<MenuItem> = (0..TOP_ACCOUNT_ROWS)
        .map(|_| MenuItem::new("Account: loading", false, None))
        .collect();
    for item in &top_accounts {
        top_accounts_menu
            .append(item)
            .map_err(|error| format!("failed to append tray account row: {error}"))?;
    }

    let separator_1 = MenuItem::new("", false, None);
    let separator_2 = MenuItem::new("", false, None);
    let separator_3 = MenuItem::new("", false, None);
    let menu = Menu::new();
    menu.append_items(&[
        &show_hide_item,
        &separator_1,
        &title_item,
        &net_worth_item,
        &as_of_item,
        &accounts_item,
        &connections_item,
        &top_accounts_menu,
        &separator_2,
        &graphs_item,
        &settings_item,
        &refresh_item,
        &separator_3,
        &quit_item,
    ])
    .map_err(|error| format!("failed to append keepbook tray menu: {error}"))?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("Keepbook")
        .with_icon(load_tray_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("failed to build keepbook tray icon: {error}"))?;

    Ok(TrayState {
        tray,
        title_item,
        net_worth_item,
        accounts_item,
        connections_item,
        as_of_item,
        top_accounts,
    })
}

fn update_tray_state(state: &TrayState, overview: Option<&Overview>) {
    let Some(overview) = overview else {
        state.title_item.set_text("Keepbook");
        state.net_worth_item.set_text("Net worth: loading");
        state.accounts_item.set_text("Accounts: loading");
        state.connections_item.set_text("Connections: loading");
        state.as_of_item.set_text("As of: loading");
        for item in &state.top_accounts {
            item.set_text("Account: loading");
            item.set_enabled(false);
        }
        return;
    };

    let total = current_net_worth_from_snapshot(&overview.snapshot);
    let total_label = format_full_money(total, &overview.reporting_currency);
    let active_accounts = overview
        .accounts
        .iter()
        .filter(|account| account.active)
        .count();
    let tooltip = format!(
        "Keepbook\nNet worth: {total_label}\nAs of: {}",
        overview.snapshot.as_of_date
    );

    state.title_item.set_text("Keepbook");
    state
        .net_worth_item
        .set_text(format!("Net worth: {total_label}"));
    state
        .as_of_item
        .set_text(format!("As of: {}", overview.snapshot.as_of_date));
    state.accounts_item.set_text(format!(
        "Accounts: {active_accounts}/{} active",
        overview.accounts.len()
    ));
    state
        .connections_item
        .set_text(format!("Connections: {}", overview.connections.len()));
    let _ = state.tray.set_tooltip(Some(tooltip));
    state.tray.set_title(Some(short_title(&total_label)));
    update_top_accounts(
        &state.top_accounts,
        &overview.snapshot.by_account,
        &overview.snapshot.currency,
    );
}

fn update_top_accounts(items: &[MenuItem], accounts: &[AccountSummary], currency: &str) {
    let mut account_rows = accounts
        .iter()
        .filter_map(|account| {
            let value = account
                .value_in_base
                .as_deref()
                .and_then(|raw| raw.parse::<f64>().ok())?;
            Some((account, value))
        })
        .collect::<Vec<_>>();
    account_rows.sort_by(|(_, left), (_, right)| {
        right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (index, item) in items.iter().enumerate() {
        if let Some((account, value)) = account_rows.get(index) {
            item.set_text(format!(
                "{}: {}",
                account.account_name,
                format_full_money(*value, currency)
            ));
            item.set_enabled(false);
        } else if index == 0 {
            item.set_text("No account breakdown available");
            item.set_enabled(false);
        } else {
            item.set_text("");
            item.set_enabled(false);
        }
    }
}

fn short_title(label: &str) -> String {
    label
        .replace('$', "")
        .replace(',', "")
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
