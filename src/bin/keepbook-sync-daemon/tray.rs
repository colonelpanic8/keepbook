use chrono::{DateTime, Local};
use ksni::menu::*;
use ksni::MenuItem;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

// --- Embedded icon PNGs (compiled into the binary) ---
const ICON_32_PNG: &[u8] = include_bytes!("../../../assets/keepbook-icon-32.png");
const ICON_48_PNG: &[u8] = include_bytes!("../../../assets/keepbook-icon-48.png");
const ICON_64_PNG: &[u8] = include_bytes!("../../../assets/keepbook-icon-64.png");

const OVERLAY_SYNC_32: &[u8] = include_bytes!("../../../assets/overlay-sync-32.png");
const OVERLAY_SYNC_48: &[u8] = include_bytes!("../../../assets/overlay-sync-48.png");
const OVERLAY_SYNC_64: &[u8] = include_bytes!("../../../assets/overlay-sync-64.png");

const OVERLAY_ERROR_32: &[u8] = include_bytes!("../../../assets/overlay-error-32.png");
const OVERLAY_ERROR_48: &[u8] = include_bytes!("../../../assets/overlay-error-48.png");
const OVERLAY_ERROR_64: &[u8] = include_bytes!("../../../assets/overlay-error-64.png");

fn png_to_argb32(png_data: &[u8]) -> ksni::Icon {
    let img = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
        .expect("embedded PNG is valid")
        .into_rgba8();
    let width = img.width() as i32;
    let height = img.height() as i32;
    // Convert RGBA → ARGB (network byte order for StatusNotifierItem)
    let data: Vec<u8> = img
        .pixels()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    ksni::Icon {
        width,
        height,
        data,
    }
}

fn load_icon_set(png_32: &[u8], png_48: &[u8], png_64: &[u8]) -> Vec<ksni::Icon> {
    vec![
        png_to_argb32(png_32),
        png_to_argb32(png_48),
        png_to_argb32(png_64),
    ]
}

struct OverlayIcons {
    sync: Vec<ksni::Icon>,
    error: Vec<ksni::Icon>,
}

impl std::fmt::Debug for OverlayIcons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayIcons").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DaemonStatus {
    Idle,
    Syncing,
    Error(String),
}

#[derive(Debug, Clone)]
pub(crate) enum DaemonCommand {
    SyncNow,
    OpenDioxusApp,
    Quit,
}

#[derive(Debug, Clone)]
pub(crate) struct KeepbookTrayState {
    pub(crate) status: DaemonStatus,
    pub(crate) last_cycle: Option<DateTime<Local>>,
    pub(crate) next_cycle: Option<DateTime<Local>>,
    pub(crate) last_summary: String,
    pub(crate) history_lines: Vec<String>,
    pub(crate) portfolio_breakdown_lines: Vec<String>,
    pub(crate) spending_lines: Vec<String>,
    pub(crate) transaction_lines: Vec<String>,
}

impl Default for KeepbookTrayState {
    fn default() -> Self {
        Self {
            status: DaemonStatus::Idle,
            last_cycle: None,
            next_cycle: None,
            last_summary: "No sync cycle has run yet".to_string(),
            history_lines: vec!["No portfolio history loaded".to_string()],
            portfolio_breakdown_lines: vec!["No portfolio breakdown loaded".to_string()],
            spending_lines: vec!["Spending metrics not loaded".to_string()],
            transaction_lines: vec!["Transactions not loaded".to_string()],
        }
    }
}

impl KeepbookTrayState {
    fn status_text(&self) -> String {
        match &self.status {
            DaemonStatus::Idle => "Idle".to_string(),
            DaemonStatus::Syncing => "Syncing...".to_string(),
            DaemonStatus::Error(msg) => format!("Error: {msg}"),
        }
    }

    fn last_cycle_text(&self) -> String {
        match self.last_cycle {
            Some(ts) => format!("Last cycle: {}", ts.format("%Y-%m-%d %H:%M:%S %Z")),
            None => "Last cycle: never".to_string(),
        }
    }

    fn next_cycle_text(&self) -> String {
        match self.next_cycle {
            Some(ts) => format!("Next cycle: {}", ts.format("%Y-%m-%d %H:%M:%S %Z")),
            None => "Next cycle: unscheduled".to_string(),
        }
    }
}

pub(crate) struct KeepbookTray {
    state: KeepbookTrayState,
    cmd_tx: UnboundedSender<DaemonCommand>,
    icons: Vec<ksni::Icon>,
    overlays: OverlayIcons,
    icon_generation: u64,
}

impl KeepbookTray {
    pub(crate) fn new(state: KeepbookTrayState, cmd_tx: UnboundedSender<DaemonCommand>) -> Self {
        Self {
            state,
            cmd_tx,
            icons: load_icon_set(ICON_32_PNG, ICON_48_PNG, ICON_64_PNG),
            overlays: OverlayIcons {
                sync: load_icon_set(OVERLAY_SYNC_32, OVERLAY_SYNC_48, OVERLAY_SYNC_64),
                error: load_icon_set(OVERLAY_ERROR_32, OVERLAY_ERROR_48, OVERLAY_ERROR_64),
            },
            icon_generation: 0,
        }
    }

    fn bump_icon_generation(&mut self) {
        self.icon_generation = self.icon_generation.wrapping_add(1);
    }

    fn overlay_pixmaps_for_status(&self) -> Vec<ksni::Icon> {
        let base = match &self.state.status {
            DaemonStatus::Idle => return self.generation_only_pixmap(),
            DaemonStatus::Syncing => &self.overlays.sync,
            DaemonStatus::Error(_) => &self.overlays.error,
        };
        let mut icons = base.clone();
        icons.push(self.generation_pixel());
        icons
    }

    fn generation_pixel(&self) -> ksni::Icon {
        let gen = self.icon_generation;
        let r = (gen & 0xFF) as u8;
        let g = ((gen >> 8) & 0xFF) as u8;
        let b = ((gen >> 16) & 0xFF) as u8;
        ksni::Icon {
            width: 1,
            height: 1,
            data: vec![0, r, g, b],
        }
    }

    fn generation_only_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.generation_pixel()]
    }
}

impl ksni::Tray for KeepbookTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "keepbook-sync-daemon".to_string()
    }

    fn title(&self) -> String {
        "keepbook sync daemon".to_string()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn overlay_icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.overlay_pixmaps_for_status()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "keepbook sync daemon".to_string(),
            description: format!(
                "{}\n{}\n{}\n{}",
                self.state.status_text(),
                self.state.last_cycle_text(),
                self.state.next_cycle_text(),
                self.state.last_summary,
            ),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let history_menu: Vec<MenuItem<Self>> = if self.state.history_lines.is_empty() {
            vec![StandardItem {
                label: "No portfolio history available".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .history_lines
                .iter()
                .map(|line| {
                    StandardItem {
                        label: line.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        let spending_items: Vec<MenuItem<Self>> = if self.state.spending_lines.is_empty() {
            vec![StandardItem {
                label: "No spending metrics available".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .spending_lines
                .iter()
                .map(|line| {
                    StandardItem {
                        label: line.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        let portfolio_breakdown_menu: Vec<MenuItem<Self>> =
            if self.state.portfolio_breakdown_lines.is_empty() {
                vec![StandardItem {
                    label: "No portfolio breakdown available".to_string(),
                    enabled: false,
                    ..Default::default()
                }
                .into()]
            } else {
                self.state
                    .portfolio_breakdown_lines
                    .iter()
                    .map(|line| {
                        StandardItem {
                            label: line.clone(),
                            enabled: false,
                            ..Default::default()
                        }
                        .into()
                    })
                    .collect()
            };

        let mut items = vec![
            StandardItem {
                label: "keepbook sync daemon".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: format!("Status: {}", self.state.status_text()),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.last_cycle_text(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.next_cycle_text(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.last_summary.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Recent Portfolio History".to_string(),
                icon_name: "view-calendar-timeline".to_string(),
                submenu: history_menu,
                ..Default::default()
            }
            .into(),
        ];

        items.push(
            SubMenu {
                label: "Portfolio Breakdown".to_string(),
                icon_name: "view-financial-account".to_string(),
                submenu: portfolio_breakdown_menu,
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Recent Spending".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );

        // Keep spending metrics as top-level rows (not nested in a submenu).
        items.extend(spending_items);

        let transaction_menu: Vec<MenuItem<Self>> = if self.state.transaction_lines.is_empty() {
            vec![StandardItem {
                label: "No recent transactions".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .transaction_lines
                .iter()
                .map(|line| {
                    StandardItem {
                        label: line.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        items.push(
            SubMenu {
                label: "Recent Transactions".to_string(),
                submenu: transaction_menu,
                ..Default::default()
            }
            .into(),
        );

        items.extend([
            MenuItem::Separator,
            StandardItem {
                label: "Sync Now".to_string(),
                icon_name: "view-refresh".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::SyncNow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Dioxus App".to_string(),
                icon_name: "keepbook".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::OpenDioxusApp);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]);

        items
    }
}

pub(crate) async fn apply_tray_state(
    tray_handle: &mut Option<ksni::Handle<KeepbookTray>>,
    state: &KeepbookTrayState,
) {
    let Some(handle) = tray_handle.as_ref() else {
        return;
    };

    let new_state = state.clone();
    let update_result = handle
        .update(move |tray: &mut KeepbookTray| {
            tray.state = new_state;
            tray.bump_icon_generation();
        })
        .await;

    if update_result.is_none() {
        warn!("Tray update failed; disabling tray updates for this process");
        *tray_handle = None;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/keepbook_sync_daemon/tray_tests.rs"]
mod tray_tests;
