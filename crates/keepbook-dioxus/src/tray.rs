use crate::{Overview, TraySnapshot};
use dioxus::desktop::{window, WindowCloseBehaviour};
use dioxus::prelude::*;
use futures_util::StreamExt;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{MenuItem, StandardItem, SubMenu};
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
const ACTIVATION_SOCKET_NAME: &str = "keepbook-dioxus.activate.sock";

#[derive(Clone, Copy, Debug)]
enum TrayCommand {
    ShowWindow,
    ToggleWindow,
    SyncNow,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayOverlayStatus {
    Idle,
    Syncing,
    Error,
}

struct TrayState {
    sender: mpsc::Sender<TrayThreadMessage>,
    #[cfg(unix)]
    activation_socket_path: Option<PathBuf>,
}

impl Drop for TrayState {
    fn drop(&mut self) {
        let _ = self.sender.send(TrayThreadMessage::Shutdown);
        #[cfg(unix)]
        if let Some(path) = &self.activation_socket_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

enum TrayThreadMessage {
    Update(Box<TrayUpdate>),
    Shutdown,
}

struct TrayUpdate {
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    runtime: TrayRuntime,
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
    win.set_minimized(false);
    win.set_visible(true);
    win.set_focus();

    #[cfg(target_os = "linux")]
    {
        let win = win.clone();
        glib::timeout_add_local_once(Duration::from_millis(75), move || {
            win.set_focus();
        });
    }
}

fn toggle_window_visibility() {
    let win = window();
    if win.is_visible() {
        win.set_visible(false);
    } else {
        show_window();
    }
}

fn quit_app(mut tray_state: Signal<Option<TrayState>>) {
    if let Some(tray_state) = tray_state.write().take() {
        let _ = tray_state.sender.send(TrayThreadMessage::Shutdown);
    }

    let win = window();
    win.set_close_behavior(WindowCloseBehaviour::WindowCloses);
    win.close();
}

#[component]
pub fn KeepbookTray(
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    runtime: TrayRuntime,
    onsyncnow: EventHandler<()>,
) -> Element {
    let mut tray_state = use_signal(|| None);
    let command_handler = use_coroutine(
        move |mut receiver: UnboundedReceiver<TrayCommand>| async move {
            while let Some(command) = receiver.next().await {
                match command {
                    TrayCommand::ShowWindow => show_window(),
                    TrayCommand::ToggleWindow => toggle_window_visibility(),
                    TrayCommand::SyncNow => {
                        onsyncnow.call(());
                    }
                    TrayCommand::Quit => {
                        quit_app(tray_state);
                        return;
                    }
                }
            }
        },
    );
    use_hook(move || {
        tray_state.set(create_tray_state(command_handler.tx()));
    });

    use_effect(use_reactive!(|overview, tray_snapshot, runtime| {
        if let Some(tray_state) = tray_state.read().as_ref() {
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
    let _ = onshowsettings;
    rsx! {}
}

fn create_tray_state(sender: UnboundedSender<TrayCommand>) -> Option<TrayState> {
    match create_tray_state_inner(sender) {
        Ok(state) => Some(state),
        Err(error) => {
            eprintln!("Failed to initialize keepbook tray icon: {error}");
            show_window();
            None
        }
    }
}

fn create_tray_state_inner(sender: UnboundedSender<TrayCommand>) -> Result<TrayState, String> {
    #[cfg(unix)]
    let activation_sender = sender.clone();
    let tray = KeepbookTrayItem::new(sender);
    let (thread_sender, thread_receiver) = mpsc::channel();
    let (handle_sender, handle_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = tray
            .spawn()
            .map_err(|error| format!("failed to build keepbook tray icon: {error}"));
        match result {
            Ok(handle) => {
                let _ = handle_sender.send(Ok(()));
                run_tray_thread(handle, thread_receiver);
            }
            Err(error) => {
                let _ = handle_sender.send(Err(error));
            }
        }
    });
    handle_receiver
        .recv()
        .map_err(|error| format!("failed to start keepbook tray thread: {error}"))??;
    #[cfg(unix)]
    let activation_socket_path = start_activation_listener(activation_sender);

    Ok(TrayState {
        sender: thread_sender,
        #[cfg(unix)]
        activation_socket_path,
    })
}

#[cfg(unix)]
pub fn activate_existing_instance() -> bool {
    match UnixStream::connect(activation_socket_path()) {
        Ok(_) => true,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound
                && error.kind() != io::ErrorKind::ConnectionRefused
            {
                eprintln!("Failed to contact existing keepbook Dioxus app: {error}");
            }
            false
        }
    }
}

#[cfg(not(unix))]
pub fn activate_existing_instance() -> bool {
    false
}

#[cfg(unix)]
fn activation_socket_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join(ACTIVATION_SOCKET_NAME);
    }

    std::env::temp_dir().join(ACTIVATION_SOCKET_NAME)
}

#[cfg(unix)]
fn start_activation_listener(sender: UnboundedSender<TrayCommand>) -> Option<PathBuf> {
    let path = activation_socket_path();

    if path.exists() {
        match UnixStream::connect(&path) {
            Ok(_) => {
                eprintln!("Keepbook activation socket already belongs to a running app.");
                return None;
            }
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionRefused
                    || error.kind() == io::ErrorKind::NotFound =>
            {
                let _ = std::fs::remove_file(&path);
            }
            Err(error) => {
                eprintln!("Failed to inspect keepbook activation socket: {error}");
                return None;
            }
        }
    }

    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Failed to bind keepbook activation socket: {error}");
            return None;
        }
    };

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(_) => {
                    let _ = sender.unbounded_send(TrayCommand::ShowWindow);
                }
                Err(error) => {
                    eprintln!("Keepbook activation listener failed: {error}");
                    break;
                }
            }
        }
    });

    Some(path)
}

fn run_tray_thread(handle: Handle<KeepbookTrayItem>, receiver: mpsc::Receiver<TrayThreadMessage>) {
    for message in receiver {
        match message {
            TrayThreadMessage::Update(update) => {
                let _ = handle.update(move |tray| {
                    tray.overview = update.overview;
                    tray.tray_snapshot = update.tray_snapshot;
                    tray.runtime = update.runtime;
                    tray.bump_icon_generation();
                });
            }
            TrayThreadMessage::Shutdown => {
                handle.shutdown().wait();
                return;
            }
        }
    }
}

fn update_tray_state(
    state: &TrayState,
    overview: Option<&Overview>,
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
    runtime: &TrayRuntime,
) {
    let overview = overview.cloned();
    let tray_snapshot = tray_snapshot.cloned();
    let runtime = runtime.clone();
    let _ = state
        .sender
        .send(TrayThreadMessage::Update(Box::new(TrayUpdate {
            overview,
            tray_snapshot,
            runtime,
        })));
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

fn disabled_item<T>(label: impl Into<String>) -> MenuItem<T> {
    StandardItem {
        label: label.into(),
        enabled: false,
        ..Default::default()
    }
    .into()
}

fn action_item(
    label: &str,
    command: TrayCommand,
    sender: &UnboundedSender<TrayCommand>,
) -> MenuItem<KeepbookTrayItem> {
    let sender = sender.clone();
    StandardItem {
        label: label.to_string(),
        activate: Box::new(move |_| {
            let _ = sender.unbounded_send(command);
        }),
        ..Default::default()
    }
    .into()
}

fn submenu(label: &str, lines: &[String]) -> MenuItem<KeepbookTrayItem> {
    SubMenu {
        label: label.to_string(),
        submenu: lines
            .iter()
            .map(|line| disabled_item(line.clone()))
            .collect(),
        ..Default::default()
    }
    .into()
}

fn png_to_argb32(png_data: &[u8]) -> ksni::Icon {
    let image = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
        .expect("embedded PNG is valid")
        .into_rgba8();
    let width = image.width() as i32;
    let height = image.height() as i32;
    let data = image
        .pixels()
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect();
    ksni::Icon {
        width,
        height,
        data,
    }
}

fn load_icon_set() -> Vec<ksni::Icon> {
    vec![
        png_to_argb32(include_bytes!("../../../assets/keepbook-icon-32.png")),
        png_to_argb32(include_bytes!("../../../assets/keepbook-icon-48.png")),
        png_to_argb32(include_bytes!("../../../assets/keepbook-icon-64.png")),
    ]
}

fn load_sync_overlay_icon_set() -> Vec<ksni::Icon> {
    vec![
        png_to_argb32(include_bytes!("../../../assets/overlay-sync-32.png")),
        png_to_argb32(include_bytes!("../../../assets/overlay-sync-48.png")),
        png_to_argb32(include_bytes!("../../../assets/overlay-sync-64.png")),
    ]
}

fn load_error_overlay_icon_set() -> Vec<ksni::Icon> {
    vec![
        png_to_argb32(include_bytes!("../../../assets/overlay-error-32.png")),
        png_to_argb32(include_bytes!("../../../assets/overlay-error-48.png")),
        png_to_argb32(include_bytes!("../../../assets/overlay-error-64.png")),
    ]
}

fn overlay_status_from_runtime(
    runtime: &TrayRuntime,
    tray_snapshot: Option<&Result<TraySnapshot, String>>,
) -> TrayOverlayStatus {
    if runtime.status_text.starts_with("Error:") || matches!(tray_snapshot, Some(Err(_))) {
        TrayOverlayStatus::Error
    } else if runtime.status_text.starts_with("Refreshing")
        || runtime.status_text.starts_with("Syncing")
    {
        TrayOverlayStatus::Syncing
    } else {
        TrayOverlayStatus::Idle
    }
}

struct KeepbookTrayItem {
    overview: Option<Overview>,
    tray_snapshot: Option<Result<TraySnapshot, String>>,
    runtime: TrayRuntime,
    sender: UnboundedSender<TrayCommand>,
    icons: Vec<ksni::Icon>,
    sync_overlays: Vec<ksni::Icon>,
    error_overlays: Vec<ksni::Icon>,
    icon_generation: u64,
}

impl KeepbookTrayItem {
    fn new(sender: UnboundedSender<TrayCommand>) -> Self {
        Self {
            overview: None,
            tray_snapshot: None,
            runtime: TrayRuntime::default(),
            sender,
            icons: load_icon_set(),
            sync_overlays: load_sync_overlay_icon_set(),
            error_overlays: load_error_overlay_icon_set(),
            icon_generation: 0,
        }
    }

    fn bump_icon_generation(&mut self) {
        self.icon_generation = self.icon_generation.wrapping_add(1);
    }

    fn generation_pixel(&self) -> ksni::Icon {
        let generation = self.icon_generation;
        let r = (generation & 0xFF) as u8;
        let g = ((generation >> 8) & 0xFF) as u8;
        let b = ((generation >> 16) & 0xFF) as u8;
        ksni::Icon {
            width: 1,
            height: 1,
            data: vec![0, r, g, b],
        }
    }

    fn overlay_icon_pixmaps(&self) -> Vec<ksni::Icon> {
        let mut icons =
            match overlay_status_from_runtime(&self.runtime, self.tray_snapshot.as_ref()) {
                TrayOverlayStatus::Idle => Vec::new(),
                TrayOverlayStatus::Syncing => self.sync_overlays.clone(),
                TrayOverlayStatus::Error => self.error_overlays.clone(),
            };
        icons.push(self.generation_pixel());
        icons
    }
}

impl ksni::Tray for KeepbookTrayItem {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "keepbook-dioxus".to_string()
    }

    fn title(&self) -> String {
        "keepbook-dioxus".to_string()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn overlay_icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.overlay_icon_pixmaps()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Keepbook".to_string(),
            description: tray_tooltip(self.tray_snapshot.as_ref(), &self.runtime),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.unbounded_send(TrayCommand::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let (history_lines, breakdown_lines, spending_lines, transaction_lines) =
            tray_lines(self.tray_snapshot.as_ref(), self.overview.as_ref());

        let mut items = vec![
            disabled_item("keepbook"),
            MenuItem::Separator,
            disabled_item(format!("Status: {}", self.runtime.status_text)),
            disabled_item(self.runtime.last_cycle_text.clone()),
            disabled_item(self.runtime.next_cycle_text.clone()),
            disabled_item(self.runtime.last_summary.clone()),
            MenuItem::Separator,
            submenu("Recent Portfolio History", &history_lines),
            submenu("Portfolio Breakdown", &breakdown_lines),
            disabled_item("Recent Spending"),
        ];

        items.extend(
            spending_lines
                .iter()
                .map(|line| disabled_item(line.clone())),
        );
        items.push(submenu("Recent Transactions", &transaction_lines));
        items.extend([
            MenuItem::Separator,
            action_item("Sync Prices", TrayCommand::SyncNow, &self.sender),
            action_item("Show/Hide Window", TrayCommand::ToggleWindow, &self.sender),
            MenuItem::Separator,
            action_item("Quit", TrayCommand::Quit, &self.sender),
        ]);

        items
    }
}

#[cfg(test)]
#[path = "../tests/unit/tray_tests.rs"]
mod tray_tests;
