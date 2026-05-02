use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
mod tray;

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_PACKAGE_DATA_DIR: &str = "/data/user/0/org.colonelpanic.keepbook.dioxus";

const APP_CSS: &str = include_str!("../assets/styles.css");
const SSH_KEY_FILE_PICKER_BRIDGE_JS: &str = r#"
(function () {
  if (window.__keepbookSshKeyPickerBridgeInstalled) {
    return;
  }
  window.__keepbookSshKeyPickerBridgeInstalled = true;
  var maxKeyBytes = 65536;

  function emitPayload(payload) {
    var sink = document.getElementById("ssh-private-key-file-payload");
    if (!sink) {
      return;
    }
    sink.value = "";
    sink.value = JSON.stringify(payload);
    sink.dispatchEvent(new Event("input", { bubbles: true }));
  }

  document.addEventListener("click", function (event) {
    var target = event.target;
    if (
      target instanceof HTMLInputElement &&
      target.id === "ssh-private-key-file-input" &&
      target.type === "file"
    ) {
      event.stopImmediatePropagation();
    }
  }, true);

  document.addEventListener("change", function (event) {
    var target = event.target;
    if (
      !(target instanceof HTMLInputElement) ||
      target.id !== "ssh-private-key-file-input" ||
      target.type !== "file"
    ) {
      return;
    }

    var file = target.files && target.files[0];
    if (!file) {
      emitPayload({ error: "No SSH key file selected." });
      return;
    }
    if (file.size > maxKeyBytes) {
      emitPayload({ error: "SSH key file is too large. Pick a private key file under 64 KB." });
      return;
    }

    emitPayload({ status: "Reading SSH key file " + file.name + "..." });

    var reader = new FileReader();
    reader.onload = function () {
      emitPayload({
        name: file.name,
        contents: String(reader.result || "")
      });
    };
    reader.onerror = function () {
      emitPayload({ error: "Key file read failed." });
    };
    reader.readAsText(file);
  }, true);
})();
"#;
#[cfg(target_arch = "wasm32")]
const API_BASE: &str = "http://127.0.0.1:8799";
const DEFAULT_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SAMPLING_GRANULARITY: SamplingGranularity = SamplingGranularity::Weekly;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Overview {
    config_path: String,
    data_dir: String,
    reporting_currency: String,
    history_defaults: HistoryDefaults,
    #[serde(default)]
    filtering: FilteringSettings,
    connections: Vec<Connection>,
    accounts: Vec<Account>,
    balances: Vec<Balance>,
    snapshot: PortfolioSnapshot,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct HistoryDefaults {
    portfolio_granularity: String,
    change_points_granularity: String,
    include_prices: bool,
    graph_range: String,
    graph_granularity: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
struct FilteringSettings {
    latent_capital_gains_tax: LatentCapitalGainsTaxFilter,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LatentCapitalGainsTaxFilter {
    configured_enabled: bool,
    effective_enabled: bool,
    override_enabled: Option<bool>,
    rate_configured: bool,
    account_name: String,
}

impl Default for LatentCapitalGainsTaxFilter {
    fn default() -> Self {
        Self {
            configured_enabled: false,
            effective_enabled: false,
            override_enabled: None,
            rate_configured: false,
            account_name: "Latent Capital Gains Tax".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FilterOverrides {
    include_latent_capital_gains_tax: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Connection {
    id: String,
    name: String,
    synchronizer: String,
    status: String,
    account_count: usize,
    last_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Account {
    id: String,
    name: String,
    connection_id: String,
    tags: Vec<String>,
    active: bool,
    #[serde(default)]
    exclude_from_portfolio: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Balance {
    account_id: String,
    asset: serde_json::Value,
    amount: String,
    value_in_reporting_currency: Option<String>,
    reporting_currency: String,
    timestamp: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ProposedTransactionEdit {
    id: String,
    account_id: String,
    account_name: String,
    transaction_id: String,
    transaction_description: String,
    transaction_timestamp: String,
    transaction_amount: String,
    created_at: String,
    updated_at: String,
    status: String,
    patch: ProposedTransactionEditPatch,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
struct ProposedTransactionEditPatch {
    description: Option<Option<String>>,
    note: Option<Option<String>>,
    category: Option<Option<String>>,
    tags: Option<Option<Vec<String>>>,
    effective_date: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct History {
    currency: String,
    points: Vec<HistoryPoint>,
    summary: Option<HistorySummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct HistoryPoint {
    date: String,
    total_value: String,
    percentage_change_from_previous: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct HistorySummary {
    initial_value: String,
    final_value: String,
    absolute_change: String,
    percentage_change: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct PortfolioSnapshot {
    as_of_date: String,
    currency: String,
    total_value: String,
    #[serde(default)]
    by_account: Vec<AccountSummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AccountSummary {
    account_id: String,
    account_name: String,
    connection_name: String,
    value_in_base: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GitRemoteSettings {
    host: String,
    repo: String,
    branch: String,
    ssh_user: String,
    #[serde(default)]
    ssh_key_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GitRepoState {
    cloned: bool,
    remote_url: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GitSettingsOutput {
    config_path: String,
    data_dir: String,
    git: GitRemoteSettings,
    repo_state: GitRepoState,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct GitSettingsInput {
    data_dir: String,
    host: String,
    repo: String,
    branch: String,
    ssh_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_key_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct GitSyncInput {
    data_dir: String,
    host: String,
    repo: String,
    branch: String,
    ssh_user: String,
    private_key_pem: String,
    save_settings: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GitSyncOutput {
    ok: bool,
    data_dir: String,
    remote_url: String,
    branch: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct SyncPricesInput {
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    force: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote_staleness_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
struct NetWorthDataPoint {
    date: String,
    value: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ChartPoint {
    date: String,
    value: f64,
    x: f64,
    y: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ChartHoverPoint {
    index: usize,
    point: ChartPoint,
    hit_x: f64,
    hit_width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangePreset {
    OneMonth,
    NinetyDays,
    SixMonths,
    OneYear,
    TwoYears,
    Max,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SamplingGranularity {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl SamplingGranularity {
    const OPTIONS: [Self; 5] = [
        Self::Auto,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Yearly => "Yearly",
        }
    }

    fn query_value(self) -> &'static str {
        match self {
            Self::Auto => "daily",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

fn range_preset_from_config(value: &str) -> RangePreset {
    match normalize_config_key(value).as_str() {
        "1m" | "1month" | "month" | "onemonth" => RangePreset::OneMonth,
        "90d" | "90days" | "ninetydays" => RangePreset::NinetyDays,
        "6m" | "6months" | "sixmonths" => RangePreset::SixMonths,
        "1y" | "1year" | "year" | "oneyear" => RangePreset::OneYear,
        "2y" | "2years" | "twoyears" => RangePreset::TwoYears,
        "max" | "all" => RangePreset::Max,
        _ => DEFAULT_RANGE_PRESET,
    }
}

fn sampling_granularity_from_config(value: &str) -> SamplingGranularity {
    match normalize_config_key(value).as_str() {
        "auto" => SamplingGranularity::Auto,
        "daily" | "day" => SamplingGranularity::Daily,
        "weekly" | "week" => SamplingGranularity::Weekly,
        "monthly" | "month" => SamplingGranularity::Monthly,
        "yearly" | "annual" | "annually" | "year" => SamplingGranularity::Yearly,
        _ => DEFAULT_SAMPLING_GRANULARITY,
    }
}

fn normalize_config_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveView {
    Summary,
    Graphs,
    Accounts,
    Connections,
    Balances,
    ProposedEdits,
    History,
    Settings,
}

impl ActiveView {
    const ALL: [Self; 8] = [
        Self::Summary,
        Self::Graphs,
        Self::Accounts,
        Self::Connections,
        Self::Balances,
        Self::ProposedEdits,
        Self::History,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Graphs => "Graphs",
            Self::Accounts => "Accounts",
            Self::Connections => "Connections",
            Self::Balances => "Balances",
            Self::ProposedEdits => "Proposed Edits",
            Self::History => "History",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum LoadState {
    Loading,
    Failed(String),
}

fn main() {
    #[cfg(feature = "desktop")]
    {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(dioxus::desktop::Config::new().with_disable_dma_buf_on_wayland(false))
            .launch(App);
    }

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    #[cfg(all(
        feature = "desktop",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    use_hook(|| {
        use dioxus::desktop::{window, WindowCloseBehaviour};
        window().set_close_behavior(WindowCloseBehaviour::WindowHides);
    });

    let mut filter_overrides = use_signal(FilterOverrides::default);
    let mut overview = use_resource(move || {
        let overrides = filter_overrides();
        async move { fetch_overview(overrides).await }
    });

    rsx! {
        DesktopTrayBridge {
            overview: overview.cloned().and_then(Result::ok),
            onrefresh: move |_| overview.restart(),
        }
        document::Title { "Keepbook" }
        document::Link { rel: "icon", href: "data:," }
        document::Style { "{APP_CSS}" }
        document::Script { "{SSH_KEY_FILE_PICKER_BRIDGE_JS}" }
        main { class: "shell",
            match overview.cloned() {
                None => rsx! { StatusPanel { state: LoadState::Loading } },
                Some(Ok(data)) => rsx! {
                    Dashboard {
                        overview: data,
                        filter_overrides: filter_overrides(),
                        onfilterchange: move |overrides| filter_overrides.set(overrides),
                        onrefresh: move |_| overview.restart()
                    }
                },
                Some(Err(error)) => rsx! {
                    StatusPanel { state: LoadState::Failed(error) }
                },
            }
        }
    }
}

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
#[component]
fn DesktopTrayBridge(overview: Option<Overview>, onrefresh: EventHandler<()>) -> Element {
    rsx! {
        tray::KeepbookTray {
            overview,
            onrefresh,
        }
    }
}

#[cfg(not(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
)))]
#[component]
fn DesktopTrayBridge(overview: Option<Overview>, onrefresh: EventHandler<()>) -> Element {
    let _ = overview;
    let _ = onrefresh;
    rsx! {}
}

async fn fetch_overview(overrides: FilterOverrides) -> Result<Overview, String> {
    fetch_overview_impl(overrides).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_overview_impl(overrides: FilterOverrides) -> Result<Overview, String> {
    let query = filter_override_query_string(overrides);
    let url = if query.is_empty() {
        format!("{API_BASE}/api/overview")
    } else {
        format!("{API_BASE}/api/overview?{query}")
    };
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Overview>()
        .await
        .map_err(|error| format!("Could not decode keepbook overview: {error}"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_overview_impl(overrides: FilterOverrides) -> Result<Overview, String> {
    let output = native_api_state()?
        .overview(keepbook_server::OverviewQuery {
            history_start: None,
            history_end: None,
            history_granularity: None,
            include_prices: None,
            include_latent_capital_gains_tax: overrides.include_latent_capital_gains_tax,
            include_history: false,
        })
        .await
        .map_err(|error| format!("Could not load keepbook overview: {error:#}"))?;
    from_native_output(output, "keepbook overview")
}

async fn fetch_history(query: String) -> Result<History, String> {
    fetch_history_impl(query).await
}

async fn fetch_git_settings() -> Result<GitSettingsOutput, String> {
    fetch_git_settings_impl().await
}

async fn save_git_settings(input: GitSettingsInput) -> Result<GitSettingsOutput, String> {
    save_git_settings_impl(input).await
}

async fn sync_git_repo(input: GitSyncInput) -> Result<GitSyncOutput, String> {
    sync_git_repo_impl(input).await
}

async fn sync_prices(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    sync_prices_impl(input).await
}

async fn fetch_proposed_transaction_edits() -> Result<Vec<ProposedTransactionEdit>, String> {
    fetch_proposed_transaction_edits_impl().await
}

async fn decide_proposed_transaction_edit(id: String, action: &'static str) -> Result<(), String> {
    decide_proposed_transaction_edit_impl(id, action).await
}

#[cfg(target_arch = "wasm32")]
async fn fetch_history_impl(query: String) -> Result<History, String> {
    let response = Request::get(&format!("{API_BASE}/api/portfolio/history?{query}"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<History>()
        .await
        .map_err(|error| format!("Could not decode net worth history: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_git_settings_impl() -> Result<GitSettingsOutput, String> {
    let response = Request::get(&format!("{API_BASE}/api/git/settings"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<GitSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode Git settings: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn save_git_settings_impl(input: GitSettingsInput) -> Result<GitSettingsOutput, String> {
    let response = Request::put(&format!("{API_BASE}/api/git/settings"))
        .json(&input)
        .map_err(|error| format!("Could not encode Git settings: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<GitSettingsOutput>()
        .await
        .map_err(|error| format!("Could not decode Git settings: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn sync_git_repo_impl(input: GitSyncInput) -> Result<GitSyncOutput, String> {
    let response = Request::post(&format!("{API_BASE}/api/git/sync"))
        .json(&input)
        .map_err(|error| format!("Could not encode Git sync request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<GitSyncOutput>()
        .await
        .map_err(|error| format!("Could not decode Git sync result: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn sync_prices_impl(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    let response = Request::post(&format!("{API_BASE}/api/sync/prices"))
        .json(&input)
        .map_err(|error| format!("Could not encode price sync request: {error}"))?
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| format!("Could not decode price sync result: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_proposed_transaction_edits_impl() -> Result<Vec<ProposedTransactionEdit>, String> {
    let response = Request::get(&format!("{API_BASE}/api/proposed-transaction-edits"))
        .send()
        .await
        .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        return Err(format!(
            "keepbook-server returned HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    response
        .json::<Vec<ProposedTransactionEdit>>()
        .await
        .map_err(|error| format!("Could not decode proposed edits: {error}"))
}

#[cfg(target_arch = "wasm32")]
async fn decide_proposed_transaction_edit_impl(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    let response = Request::post(&format!(
        "{API_BASE}/api/proposed-transaction-edits/{id}/{action}"
    ))
    .send()
    .await
    .map_err(|error| format!("Could not reach keepbook-server at {API_BASE}: {error}"))?;

    if !response.ok() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("keepbook-server returned HTTP {status}: {text}"));
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_history_impl(query: String) -> Result<History, String> {
    let query = serde_urlencoded::from_str::<keepbook_server::HistoryQuery>(&query)
        .map_err(|error| format!("Could not encode history query: {error}"))?;
    let output = native_api_state()?
        .portfolio_history(query)
        .await
        .map_err(|error| format!("Could not load net worth history: {error:#}"))?;
    from_native_output(output, "net worth history")
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_git_settings_impl() -> Result<GitSettingsOutput, String> {
    let output = native_api_state()?
        .git_settings()
        .await
        .map_err(|error| format!("Could not load Git settings: {error:#}"))?;
    from_native_output(output, "Git settings")
}

#[cfg(not(target_arch = "wasm32"))]
async fn save_git_settings_impl(input: GitSettingsInput) -> Result<GitSettingsOutput, String> {
    let output = native_api_state()?
        .save_git_settings(keepbook_server::GitSettingsInput {
            data_dir: input.data_dir,
            host: input.host,
            repo: input.repo,
            branch: input.branch,
            ssh_user: input.ssh_user,
            ssh_key_path: input.ssh_key_path,
        })
        .await
        .map_err(|error| format!("Could not save Git settings: {error:#}"))?;
    from_native_output(output, "Git settings")
}

#[cfg(not(target_arch = "wasm32"))]
async fn sync_git_repo_impl(input: GitSyncInput) -> Result<GitSyncOutput, String> {
    let output = native_api_state()?
        .sync_git_repo(keepbook_server::GitSyncInput {
            data_dir: input.data_dir,
            host: input.host,
            repo: input.repo,
            branch: input.branch,
            ssh_user: input.ssh_user,
            private_key_pem: input.private_key_pem,
            save_settings: input.save_settings,
        })
        .await
        .map_err(|error| format!("Sync failed: {error:#}"))?;
    from_native_output(output, "Git sync result")
}

#[cfg(not(target_arch = "wasm32"))]
async fn sync_prices_impl(input: SyncPricesInput) -> Result<serde_json::Value, String> {
    native_api_state()?
        .sync_prices(keepbook_server::SyncPricesInput {
            scope: Some(input.scope),
            target: input.target,
            force: input.force,
            quote_staleness_seconds: input.quote_staleness_seconds,
        })
        .await
        .map_err(|error| format!("Price sync failed: {error:#}"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_proposed_transaction_edits_impl() -> Result<Vec<ProposedTransactionEdit>, String> {
    let output = native_api_state()?
        .proposed_transaction_edits(keepbook_server::ProposedTransactionEditsQuery {
            include_decided: false,
        })
        .await
        .map_err(|error| format!("Could not load proposed edits: {error:#}"))?;
    from_native_output(output, "proposed edits")
}

#[cfg(not(target_arch = "wasm32"))]
async fn decide_proposed_transaction_edit_impl(
    id: String,
    action: &'static str,
) -> Result<(), String> {
    let state = native_api_state()?;
    let result = match action {
        "approve" => state.approve_proposed_transaction_edit(id).await,
        "reject" => state.reject_proposed_transaction_edit(id).await,
        "remove" => state.remove_proposed_transaction_edit(id).await,
        _ => return Err(format!("Unsupported proposal action: {action}")),
    };
    result
        .map(|_| ())
        .map_err(|error| format!("Could not update proposed edit: {error:#}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn native_api_state() -> Result<&'static keepbook_server::ApiState, String> {
    static STATE: OnceLock<keepbook_server::ApiState> = OnceLock::new();
    if let Some(state) = STATE.get() {
        return Ok(state);
    }

    let state = keepbook_server::ApiState::load(native_config_path())
        .map_err(|error| format!("Could not initialize local keepbook API: {error:#}"))?;
    let _ = STATE.set(state);
    STATE
        .get()
        .ok_or_else(|| "Could not initialize local keepbook API".to_string())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_app_files_dir() -> PathBuf {
    PathBuf::from(ANDROID_PACKAGE_DATA_DIR).join("files")
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_default_git_data_dir() -> PathBuf {
    android_app_files_dir().join("keepbook-data")
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn normalize_android_app_data_path(path: String) -> String {
    let legacy_prefix = "/data/data/org.colonelpanic.keepbook.dioxus";
    path.strip_prefix(legacy_prefix)
        .map(|suffix| format!("{ANDROID_PACKAGE_DATA_DIR}{suffix}"))
        .unwrap_or(path)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn native_config_path() -> PathBuf {
    let files_dir = android_app_files_dir();
    if let Err(error) = std::fs::create_dir_all(&files_dir) {
        eprintln!(
            "Could not create Android keepbook files dir {}: {error}",
            files_dir.display()
        );
    }

    let config_path = files_dir.join("keepbook.toml");
    if !config_path.exists() {
        let default_config = "data_dir = \"./keepbook-data\"\n";
        if let Err(error) = std::fs::write(&config_path, default_config) {
            eprintln!(
                "Could not write Android keepbook config {}: {error}",
                config_path.display()
            );
        }
    }

    config_path
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn native_config_path() -> PathBuf {
    keepbook_server::default_server_config_path()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn recommended_data_dir() -> Option<String> {
    Some(android_default_git_data_dir().display().to_string())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn normalize_git_data_dir_for_client(path: String) -> String {
    normalize_android_app_data_path(path)
}

#[cfg(any(target_arch = "wasm32", not(target_os = "android")))]
fn normalize_git_data_dir_for_client(path: String) -> String {
    path
}

#[cfg(any(target_arch = "wasm32", not(target_os = "android")))]
fn recommended_data_dir() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn from_native_output<T, U>(output: U, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    U: Serialize,
{
    serde_json::from_value(
        serde_json::to_value(output)
            .map_err(|error| format!("Could not encode {label}: {error}"))?,
    )
    .map_err(|error| format!("Could not decode {label}: {error}"))
}
#[component]
fn StatusPanel(state: LoadState) -> Element {
    let message = match state {
        LoadState::Loading => "Loading local finance data...".to_string(),
        LoadState::Failed(error) => error,
    };

    rsx! {
        section { class: "status-panel",
            h2 { "Connection" }
            p { "{message}" }
        }
    }
}

#[component]
fn Dashboard(
    overview: Overview,
    filter_overrides: FilterOverrides,
    onfilterchange: EventHandler<FilterOverrides>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut active_view = use_signal(|| ActiveView::Summary);
    let mut nav_open = use_signal(|| false);
    let active = active_view();
    let total = current_net_worth_from_snapshot(&overview.snapshot);
    let last_date = overview.snapshot.as_of_date.clone();
    let active_accounts = overview
        .accounts
        .iter()
        .filter(|account| account.active)
        .count();
    let nav_class = if nav_open() {
        "app-nav open"
    } else {
        "app-nav"
    };

    rsx! {
        div { class: "app-shell",
            DesktopTrayViewActions {
                onshowgraphs: move |_| active_view.set(ActiveView::Graphs),
                onshowsettings: move |_| active_view.set(ActiveView::Settings),
            }
            aside { class: "{nav_class}",
                div { class: "nav-title",
                    strong { "Keepbook" }
                    small { "{overview.reporting_currency}" }
                }
                nav {
                    for view in ActiveView::ALL {
                        NavButton {
                            label: view.label(),
                            selected: active == view,
                            onclick: move |_| {
                                active_view.set(view);
                                nav_open.set(false);
                            }
                        }
                    }
                }
            }
            div { class: "workspace",
                header { class: "topbar",
                    button {
                        class: "hamburger-button",
                        title: "Menu",
                        onclick: move |_| nav_open.set(!nav_open()),
                        span { class: "hamburger-line" }
                        span { class: "hamburger-line" }
                        span { class: "hamburger-line" }
                    }
                    div {
                        h1 { "{active.label()}" }
                    }
                    button {
                        class: "icon-button",
                        title: "Refresh",
                        onclick: move |_| onrefresh.call(()),
                        "Refresh"
                    }
                }
                match active {
                    ActiveView::Summary => rsx! {
                        SummaryView {
                            net_worth: total,
                            currency: overview.reporting_currency.clone(),
                            last_date: last_date.to_string(),
                            active_accounts,
                            total_accounts: overview.accounts.len(),
                            connection_count: overview.connections.len(),
                        }
                    },
                    ActiveView::Graphs => rsx! {
                        GraphsView {
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            filter_overrides,
                        }
                    },
                    ActiveView::Accounts => rsx! {
                        AccountsView {
                            accounts: overview.accounts.clone(),
                            connections: overview.connections.clone(),
                            balances: overview.balances.clone(),
                            snapshot: overview.snapshot.clone(),
                            currency: overview.reporting_currency.clone(),
                            onrefresh: move |_| onrefresh.call(()),
                        }
                    },
                    ActiveView::Connections => rsx! {
                        ConnectionsView { connections: overview.connections.clone() }
                    },
                    ActiveView::Balances => rsx! {
                        BalancesView { balances: overview.balances.clone() }
                    },
                    ActiveView::ProposedEdits => rsx! {
                        ProposedEditsView {
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                    ActiveView::History => rsx! {
                        HistoryView {
                            currency: overview.reporting_currency.clone(),
                            defaults: overview.history_defaults.clone(),
                            filter_overrides,
                        }
                    },
                    ActiveView::Settings => rsx! {
                        SettingsView {
                            filtering: overview.filtering.clone(),
                            filter_overrides,
                            config_path: overview.config_path.clone(),
                            data_dir: overview.data_dir.clone(),
                            onfilterchange,
                            onrefresh: move |_| onrefresh.call(())
                        }
                    },
                }
            }
        }
    }
}

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
#[component]
fn DesktopTrayViewActions(
    onshowgraphs: EventHandler<()>,
    onshowsettings: EventHandler<()>,
) -> Element {
    rsx! {
        tray::TrayViewActions {
            onshowgraphs,
            onshowsettings,
        }
    }
}

#[cfg(not(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
)))]
#[component]
fn DesktopTrayViewActions(
    onshowgraphs: EventHandler<()>,
    onshowsettings: EventHandler<()>,
) -> Element {
    let _ = onshowgraphs;
    let _ = onshowsettings;
    rsx! {}
}

#[component]
fn NavButton(label: &'static str, selected: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if selected {
        "nav-button selected"
    } else {
        "nav-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn SummaryView(
    net_worth: f64,
    currency: String,
    last_date: String,
    active_accounts: usize,
    total_accounts: usize,
    connection_count: usize,
) -> Element {
    rsx! {
        section { class: "summary-grid",
            MetricCard {
                label: "Net worth",
                value: format_full_money(net_worth, &currency),
                detail: last_date
            }
            MetricCard {
                label: "Accounts",
                value: active_accounts.to_string(),
                detail: format!("{total_accounts} total")
            }
            MetricCard {
                label: "Connections",
                value: connection_count.to_string(),
                detail: "Configured sources".to_string()
            }
        }
    }
}

#[component]
fn GraphsView(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    rsx! {
        section { class: "panel graph-panel",
            NetWorthPanel { currency, defaults, filter_overrides }
        }
    }
}

#[component]
fn PortfolioSettingsPanel(
    filtering: FilteringSettings,
    filter_overrides: FilterOverrides,
    config_path: String,
    data_dir: String,
    onfilterchange: EventHandler<FilterOverrides>,
) -> Element {
    let latent_tax = filtering.latent_capital_gains_tax;
    let override_active = filter_overrides.include_latent_capital_gains_tax.is_some();
    let source = if override_active {
        "Dioxus override"
    } else {
        "TOML default"
    };
    let configured_state = enabled_label(latent_tax.configured_enabled);
    let effective_state = enabled_label(latent_tax.effective_enabled);
    let rate_state = if latent_tax.rate_configured {
        "Configured"
    } else {
        "Missing"
    };

    rsx! {
        section { class: "panel settings-panel",
            div { class: "panel-header",
                h2 { "Portfolio" }
                span { "{source}" }
            }
            div { class: "settings-list",
                article { class: "setting-row",
                    div { class: "setting-copy",
                        strong { "Latent capital gains tax" }
                        small { "Include {latent_tax.account_name} in net worth and history" }
                    }
                    label { class: "switch-control",
                        input {
                            r#type: "checkbox",
                            checked: latent_tax.effective_enabled,
                            onchange: move |event| {
                                let mut next = filter_overrides;
                                next.include_latent_capital_gains_tax = Some(event.checked());
                                onfilterchange.call(next);
                            }
                        }
                        span { class: "switch-track",
                            span { class: "switch-thumb" }
                        }
                    }
                }
            }
            div { class: "settings-meta settings-meta-grid",
                span { "Default {configured_state}" }
                span { "Current {effective_state}" }
                span { "Tax rate {rate_state}" }
            }
            div { class: "settings-actions",
                button {
                    class: "control-button",
                    disabled: !override_active,
                    onclick: move |_| {
                        let mut next = filter_overrides;
                        next.include_latent_capital_gains_tax = None;
                        onfilterchange.call(next);
                    },
                    "Reset"
                }
            }
            div { class: "settings-source",
                small { "{config_path}" }
                small { "{data_dir}" }
            }
        }
    }
}

#[component]
fn SettingsView(
    filtering: FilteringSettings,
    filter_overrides: FilterOverrides,
    config_path: String,
    data_dir: String,
    onfilterchange: EventHandler<FilterOverrides>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut settings = use_resource(fetch_git_settings);
    let mut loaded_key = use_signal(String::new);
    let mut git_data_dir = use_signal(String::new);
    let mut host = use_signal(|| "github.com".to_string());
    let mut repo = use_signal(|| "colonelpanic8/keepbook-data".to_string());
    let mut branch = use_signal(|| "master".to_string());
    let mut ssh_user = use_signal(|| "git".to_string());
    let mut ssh_key_path = use_signal(|| None::<String>);
    let mut private_key = use_signal(String::new);
    let mut private_key_name = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut add_location_open = use_signal(|| false);
    let mut location_remote_input = use_signal(String::new);
    let mut location_path_input = use_signal(String::new);
    let mut location_branch_input = use_signal(|| "master".to_string());
    let mut location_error = use_signal(String::new);
    let mut clone_dialog_open = use_signal(|| false);
    let mut clone_dialog_title = use_signal(String::new);
    let mut clone_dialog_message = use_signal(String::new);

    if let Some(Ok(current)) = settings.cloned() {
        let key = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            current.data_dir,
            current.git.host,
            current.git.repo,
            current.git.branch,
            current.git.ssh_user,
            current.git.ssh_key_path.as_deref().unwrap_or_default()
        );
        if loaded_key() != key {
            git_data_dir.set(normalize_git_data_dir_for_client(current.data_dir));
            host.set(current.git.host);
            repo.set(current.git.repo);
            branch.set(current.git.branch);
            ssh_user.set(current.git.ssh_user);
            ssh_key_path.set(current.git.ssh_key_path);
            loaded_key.set(key);
        }
    }

    let current_settings = settings.cloned();
    let is_busy = busy();
    let status_text = status();

    rsx! {
        PortfolioSettingsPanel {
            filtering,
            filter_overrides,
            config_path,
            data_dir,
            onfilterchange,
        }
        section { class: "panel settings-panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Git Sync" }
                    span { "Server-backed" }
                }
                div { class: "settings-actions inline-actions",
                    button {
                        class: "icon-button add-location-button",
                        title: "Add location",
                        disabled: is_busy,
                        onclick: move |_| {
                            location_remote_input.set(remote_input_from_settings(&host(), &repo(), &ssh_user()));
                            location_path_input.set(git_data_dir());
                            location_branch_input.set(branch());
                            location_error.set(String::new());
                            add_location_open.set(true);
                        },
                        "+"
                    }
                }
            }
            match current_settings {
                None => rsx! { BackendActivity { message: "Loading Git settings" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(current)) => rsx! {
                    div { class: "settings-meta",
                        span { "Config {current.config_path}" }
                    }
                    if !status_text.is_empty() {
                        p { class: "settings-status", "{status_text}" }
                    }
                    GitLocationList {
                        current: current.clone(),
                        staged_data_dir: git_data_dir(),
                        staged_remote: remote_input_from_settings(&host(), &repo(), &ssh_user()),
                        staged_branch: branch(),
                        disabled: is_busy
                            || (private_key().trim().is_empty()
                                && ssh_key_path().as_deref().unwrap_or_default().trim().is_empty()),
                        onclone: move |_| {
                            let repo_cloned = current.repo_state.cloned;
                            let input = GitSyncInput {
                                data_dir: git_data_dir(),
                                host: host(),
                                repo: repo(),
                                branch: branch(),
                                ssh_user: ssh_user(),
                                private_key_pem: private_key(),
                                save_settings: true,
                            };
                            let action = if repo_cloned { "Syncing" } else { "Cloning" };
                            let key_source = if input.private_key_pem.trim().is_empty() {
                                "saved SSH key"
                            } else {
                                "selected SSH key"
                            };
                            busy.set(true);
                            clone_dialog_open.set(true);
                            clone_dialog_title.set(format!("{action} repository"));
                            clone_dialog_message.set(format!(
                                "{} {} at {} using {}",
                                action,
                                remote_input_from_settings(&input.host, &input.repo, &input.ssh_user),
                                input.data_dir,
                                key_source
                            ));
                            status.set(format!("{action} repository..."));
                            spawn(async move {
                                match sync_git_repo(input).await {
                                    Ok(result) => {
                                        clone_dialog_title.set("Repository ready".to_string());
                                        clone_dialog_message.set(format!(
                                            "Synced {} from {} {}",
                                            result.data_dir, result.remote_url, result.branch
                                        ));
                                        status.set(format!("Synced {} from {} {}", result.data_dir, result.remote_url, result.branch));
                                        settings.restart();
                                        onrefresh.call(());
                                    }
                                    Err(error) => {
                                        clone_dialog_title.set("Git operation failed".to_string());
                                        clone_dialog_message.set(error.clone());
                                        status.set(format!("Sync failed: {error}"));
                                    }
                                }
                                busy.set(false);
                            });
                        },
                    }
                    div { class: "control-field secret-field",
                        span { "SSH private key" }
                        div { class: "key-file-picker",
                            label { class: "file-select-wrapper",
                                input {
                                    id: "ssh-private-key-file-input",
                                    class: "file-select-input",
                                    r#type: "file",
                                    disabled: is_busy,
                                }
                                span { class: "file-select-button", "Select key file" }
                            }
                            input {
                                id: "ssh-private-key-file-payload",
                                class: "file-payload-input",
                                r#type: "text",
                                oninput: move |event| {
                                    match serde_json::from_str::<serde_json::Value>(&event.value()) {
                                        Ok(payload) => {
                                            if let Some(message) = payload.get("status").and_then(|value| value.as_str()) {
                                                status.set(message.to_string());
                                                return;
                                            }
                                            if let Some(error) = payload.get("error").and_then(|value| value.as_str()) {
                                                status.set(error.to_string());
                                                return;
                                            }
                                            let name = payload
                                                .get("name")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or("selected key")
                                                .to_string();
                                            let contents = payload
                                                .get("contents")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            if contents.trim().is_empty() {
                                                status.set("Selected SSH key file is empty.".to_string());
                                            } else {
                                                private_key.set(contents);
                                                private_key_name.set(name.clone());
                                                status.set(format!("Selected SSH key file: {name}."));
                                            }
                                        }
                                        Err(error) => status.set(format!("Key file read failed: {error}")),
                                    }
                                }
                            }
                            small { class: "key-file-status",
                                if private_key().trim().is_empty() {
                                    if let Some(saved_key_path) = ssh_key_path() {
                                        "Saved key: {saved_key_path}"
                                    } else {
                                        "No private key selected"
                                    }
                                } else if private_key_name().is_empty() {
                                    "Private key loaded"
                                } else {
                                    "{private_key_name()} loaded"
                                }
                            }
                            if !private_key().trim().is_empty() {
                                button {
                                    class: "control-button",
                                    disabled: is_busy,
                                    onclick: move |_| {
                                        private_key.set(String::new());
                                        private_key_name.set(String::new());
                                        status.set("SSH key cleared.".to_string());
                                    },
                                    "Clear key"
                                }
                            }
                        }
                    }
                },
            }
        }
        if add_location_open() {
            div { class: "modal-backdrop",
                div { class: "modal-dialog",
                    div { class: "modal-header",
                        h3 { "Add location" }
                        button {
                            class: "icon-button",
                            disabled: is_busy,
                            onclick: move |_| add_location_open.set(false),
                            "x"
                        }
                    }
                    label { class: "control-field",
                        span { "Remote" }
                        input {
                            class: "control-input",
                            r#type: "text",
                            value: "{location_remote_input()}",
                            placeholder: "git@github.com:owner/keepbook-data.git",
                            autofocus: true,
                            oninput: move |event| {
                                location_remote_input.set(event.value());
                                location_error.set(String::new());
                            }
                        }
                    }
                    TextInput {
                        label: "Location",
                        value: location_path_input(),
                        placeholder: "/path/to/keepbook-data",
                        oninput: move |value| location_path_input.set(value)
                    }
                    TextInput {
                        label: "Branch",
                        value: location_branch_input(),
                        placeholder: "master",
                        oninput: move |value| location_branch_input.set(value)
                    }
                    if let Some(path) = recommended_data_dir() {
                        div { class: "settings-actions inline-actions",
                            button {
                                class: "control-button",
                                disabled: is_busy,
                                onclick: move |_| location_path_input.set(path.clone()),
                                "Use app data folder"
                            }
                        }
                    }
                    if !location_error().is_empty() {
                        p { class: "validation", "{location_error()}" }
                    }
                    div { class: "modal-actions",
                        button {
                            class: "control-button",
                            disabled: is_busy,
                            onclick: move |_| add_location_open.set(false),
                            "Cancel"
                        }
                        button {
                            class: "control-button selected",
                            disabled: is_busy,
                            onclick: move |_| {
                                match git_settings_from_remote(&location_remote_input()) {
                                    Ok((next_host, next_repo, next_ssh_user)) => {
                                        let next_data_dir = location_path_input();
                                        if next_data_dir.trim().is_empty() {
                                            location_error.set("Enter a local location.".to_string());
                                            return;
                                        }
                                        let next_branch = non_empty_client(&location_branch_input(), "master");
                                        let input = GitSettingsInput {
                                            data_dir: next_data_dir.clone(),
                                            host: next_host.clone(),
                                            repo: next_repo.clone(),
                                            branch: next_branch.clone(),
                                            ssh_user: next_ssh_user.clone(),
                                            ssh_key_path: ssh_key_path(),
                                        };
                                        busy.set(true);
                                        status.set("Saving Git location...".to_string());
                                        spawn(async move {
                                            match save_git_settings(input).await {
                                                Ok(saved) => {
                                                    git_data_dir.set(normalize_git_data_dir_for_client(saved.data_dir));
                                                    host.set(saved.git.host);
                                                    repo.set(saved.git.repo);
                                                    branch.set(saved.git.branch);
                                                    ssh_user.set(saved.git.ssh_user);
                                                    ssh_key_path.set(saved.git.ssh_key_path);
                                                    location_error.set(String::new());
                                                    add_location_open.set(false);
                                                    status.set("Git location added.".to_string());
                                                    settings.restart();
                                                    onrefresh.call(());
                                                }
                                                Err(error) => {
                                                    location_error.set(error.clone());
                                                    status.set(format!("Save failed: {error}"));
                                                }
                                            }
                                            busy.set(false);
                                        });
                                    }
                                    Err(error) => location_error.set(error),
                                }
                            },
                            "Add"
                        }
                    }
                }
            }
        }
        if clone_dialog_open() {
            div { class: "modal-backdrop",
                div { class: "modal-dialog clone-dialog",
                    div { class: "modal-header",
                        h3 { "{clone_dialog_title()}" }
                        if !is_busy {
                            button {
                                class: "icon-button",
                                onclick: move |_| clone_dialog_open.set(false),
                                "x"
                            }
                        }
                    }
                    div { class: "clone-progress",
                        if is_busy {
                            span { class: "activity-spinner large" }
                        }
                        p { "{clone_dialog_message()}" }
                    }
                    if !is_busy {
                        div { class: "modal-actions",
                            button {
                                class: "control-button selected",
                                onclick: move |_| clone_dialog_open.set(false),
                                "Close"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn git_settings_from_remote(remote: &str) -> Result<(String, String, String), String> {
    let trimmed = remote.trim();
    if trimmed.is_empty() {
        return Err("Enter a remote.".to_string());
    }

    if is_explicit_git_remote(trimmed) {
        return Ok((
            remote_host(trimmed).unwrap_or_else(|| "github.com".to_string()),
            trimmed.to_string(),
            remote_user(trimmed).unwrap_or_else(|| "git".to_string()),
        ));
    }

    normalize_github_repo_input(trimmed)
        .map(|repo| ("github.com".to_string(), repo, "git".to_string()))
}

fn remote_input_from_settings(host: &str, repo: &str, ssh_user: &str) -> String {
    let repo = repo.trim();
    if repo.is_empty() {
        return String::new();
    }
    if is_explicit_git_remote(repo) {
        return repo.to_string();
    }

    let host = non_empty_client(host, "github.com");
    let ssh_user = non_empty_client(ssh_user, "git");
    let repo = if repo.ends_with(".git") {
        repo.to_string()
    } else {
        format!("{repo}.git")
    };
    format!("{ssh_user}@{host}:{repo}")
}

fn non_empty_client(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_explicit_git_remote(remote: &str) -> bool {
    remote.contains("://") || (remote.contains('@') && remote.contains(':'))
}

fn remote_user(remote: &str) -> Option<String> {
    remote
        .split('@')
        .next()
        .and_then(|prefix| prefix.rsplit(['/', ':']).next())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn remote_host(remote: &str) -> Option<String> {
    let without_scheme = remote.split("://").nth(1).unwrap_or(remote);
    let after_user = without_scheme.split('@').nth(1).unwrap_or(without_scheme);
    after_user
        .split([':', '/'])
        .next()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_github_repo_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    let repo = trim_github_repo_prefix(trimmed)
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| trim_github_repo_prefix(trimmed).trim_matches('/'));

    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    let Some(name) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    Ok(format!("{owner}/{name}"))
}

fn trim_github_repo_prefix(input: &str) -> &str {
    input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("git@github.com:"))
        .unwrap_or(input)
}

#[component]
fn GitLocationList(
    current: GitSettingsOutput,
    staged_data_dir: String,
    staged_remote: String,
    staged_branch: String,
    disabled: bool,
    onclone: EventHandler<()>,
) -> Element {
    let state = current.repo_state;
    let remote_label = state.remote_url.clone().unwrap_or(staged_remote);
    let branch_label = state.branch.clone().unwrap_or(staged_branch);
    let commit_label = state
        .commit
        .as_deref()
        .map(short_commit)
        .unwrap_or_else(|| "Not cloned".to_string());
    let status_label = if state.cloned { "Cloned" } else { "Not cloned" };
    let action_label = if state.cloned { "Sync" } else { "Clone" };

    rsx! {
        div { class: "git-locations",
            div { class: "git-locations-heading",
                strong { "Known locations" }
            }
            div { class: "git-location-row",
                div { class: "git-location-main",
                    div { class: "git-location-title",
                        strong { "{status_label}" }
                        small { "{branch_label}" }
                    }
                    div { class: "git-state-grid",
                        div { class: "git-state-row",
                            span { "Remote" }
                            code { "{remote_label}" }
                        }
                        div { class: "git-state-row",
                            span { "Commit" }
                            code { "{commit_label}" }
                        }
                        div { class: "git-state-row",
                            span { "Location" }
                            code { "{staged_data_dir}" }
                        }
                    }
                }
                div { class: "git-location-actions",
                    button {
                        class: "control-button selected",
                        disabled,
                        onclick: move |_| onclone.call(()),
                        "{action_label}"
                    }
                }
            }
        }
    }
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

#[component]
fn InlineStatus(title: &'static str, message: String) -> Element {
    rsx! {
        div { class: "inline-status",
            h2 { "{title}" }
            p { "{message}" }
        }
    }
}

#[component]
fn MetricCard(label: String, value: String, detail: String) -> Element {
    rsx! {
        article { class: "metric",
            span { class: "metric-label", "{label}" }
            strong { "{value}" }
            small { "{detail}" }
        }
    }
}

#[component]
fn NetWorthPanel(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    let initial_range_preset = range_preset_from_config(&defaults.graph_range);
    let initial_sampling_granularity =
        sampling_granularity_from_config(&defaults.graph_granularity);
    let mut range_preset = use_signal(move || initial_range_preset);
    let mut start_override = use_signal(String::new);
    let mut end_override = use_signal(String::new);
    let mut y_min_input = use_signal(String::new);
    let mut y_max_input = use_signal(String::new);
    let mut sampling_granularity = use_signal(move || initial_sampling_granularity);
    let history = use_resource(move || {
        let selected_range = range_preset();
        let start_text = start_override();
        let end_text = end_override();
        let selected_sampling = sampling_granularity();
        async move {
            fetch_history(history_query_string(
                selected_range,
                &start_text,
                &end_text,
                selected_sampling,
                &current_date_string(),
                filter_overrides,
            ))
            .await
        }
    });

    let selected_range = range_preset();
    let selected_sampling = sampling_granularity();
    let start_text = start_override();
    let end_text = end_override();
    let history_state = history.cloned();
    let is_history_loading = history_state.is_none();
    let loaded_history = match &history_state {
        Some(Ok(history)) => Some(history),
        _ => None,
    };
    let data = loaded_history.map(history_data_points).unwrap_or_default();
    let bounds = date_bounds(&data);
    let (start_date, end_date) = visible_date_range(&data, selected_range, &start_text, &end_text);
    let visible_data = filter_data_by_date_range(&data, &start_date, &end_date);
    let resolved_sampling = resolve_sampling_granularity(selected_sampling, &visible_data);
    let sampled_data = sample_data_by_granularity(&visible_data, resolved_sampling);
    let sampled_point_count = sampled_data.len();
    let sampling_label = resolved_sampling.label();
    let visible_value_bounds = value_bounds(&sampled_data);
    let y_min_text = y_min_input();
    let y_max_text = y_max_input();
    let y_domain = parse_y_domain(&y_min_text, &y_max_text);
    let has_date_error = !start_date.is_empty() && !end_date.is_empty() && start_date > end_date;
    let has_y_error = !y_min_text.is_empty() && !y_max_text.is_empty() && y_domain.is_none();
    let current_value = sampled_data
        .last()
        .map(|point| point.value)
        .unwrap_or_default();
    let start_value = sampled_data
        .first()
        .map(|point| point.value)
        .unwrap_or_default();
    let absolute_change = current_value - start_value;
    let percentage_change = if start_value == 0.0 {
        None
    } else {
        Some((absolute_change / start_value) * 100.0)
    };
    let data_y_range = visible_value_bounds
        .map(|(min, max)| {
            format!(
                "{} to {}",
                format_full_money(min, &currency),
                format_full_money(max, &currency)
            )
        })
        .unwrap_or_else(|| "No visible data".to_string());
    let axis_y_range = y_domain
        .map(|(min, max)| {
            format!(
                "{} to {}",
                format_full_money(min, &currency),
                format_full_money(max, &currency)
            )
        })
        .unwrap_or_else(|| "Auto".to_string());
    let change_class = if absolute_change >= 0.0 {
        "change-positive"
    } else {
        "change-negative"
    };
    let percent_text = percentage_change
        .map(|value| format!("{}%", format_number(value, 2)))
        .unwrap_or_else(|| "N/A".to_string());
    let min_date = bounds
        .as_ref()
        .map(|bounds| bounds.0.clone())
        .unwrap_or_default();
    let max_date = bounds
        .as_ref()
        .map(|bounds| bounds.1.clone())
        .unwrap_or_default();
    let header_currency = loaded_history
        .map(|history| history.currency.clone())
        .unwrap_or_else(|| currency.clone());

    rsx! {
        div { class: "panel-header",
            h2 { "Net Worth Over Time" }
            span { "{header_currency}" }
        }
        if is_history_loading {
            BackendActivity { message: "Waiting on backend graph data" }
        }
        div { class: "chart-controls",
            div { class: "preset-row",
                span { class: "control-label", "Range" }
                GraphPresetButton {
                    label: "1M",
                    selected: selected_range == RangePreset::OneMonth,
                    onclick: move |_| {
                        range_preset.set(RangePreset::OneMonth);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "90D",
                    selected: selected_range == RangePreset::NinetyDays,
                    onclick: move |_| {
                        range_preset.set(RangePreset::NinetyDays);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "6M",
                    selected: selected_range == RangePreset::SixMonths,
                    onclick: move |_| {
                        range_preset.set(RangePreset::SixMonths);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "1Y",
                    selected: selected_range == RangePreset::OneYear,
                    onclick: move |_| {
                        range_preset.set(RangePreset::OneYear);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "2Y",
                    selected: selected_range == RangePreset::TwoYears,
                    onclick: move |_| {
                        range_preset.set(RangePreset::TwoYears);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                GraphPresetButton {
                    label: "Max",
                    selected: selected_range == RangePreset::Max,
                    onclick: move |_| {
                        range_preset.set(RangePreset::Max);
                        start_override.set(String::new());
                        end_override.set(String::new());
                    }
                }
                button {
                    class: "control-button",
                    onclick: move |_| {
                        if let Some((min, max)) = visible_value_bounds {
                            y_min_input.set(format_input_number(min));
                            y_max_input.set(format_input_number(max));
                        }
                    },
                    "Fit Y"
                }
            }
            div { class: "sampling-row",
                span { class: "control-label", "Sampling" }
                for option in SamplingGranularity::OPTIONS {
                    GraphPresetButton {
                        label: option.label(),
                        selected: selected_sampling == option,
                        onclick: move |_| sampling_granularity.set(option)
                    }
                }
            }
        }
        match history_state {
            None => rsx! {
                GraphLoadingPanel {
                    range: range_summary_text(&start_date, &end_date),
                    sampling: selected_sampling.label()
                }
            },
            Some(Err(error)) => rsx! {
                InlineStatus { title: "Net Worth Over Time", message: error }
            },
            Some(Ok(_)) => rsx! {
                NetWorthChart {
                    data: sampled_data.clone(),
                    currency: currency.clone(),
                    y_domain
                }
                if !sampled_data.is_empty() {
                    div { class: "chart-stats",
                        strong { "{format_full_money(current_value, &currency)}" }
                        span { class: "{change_class}",
                            "{format_signed_money(absolute_change, &currency)} ({percent_text})"
                        }
                    }
                }
            }
        }
        div { class: "chart-controls chart-bottom-controls",
            div { class: "control-grid",
                DateInput {
                    label: "Start",
                    value: start_date.clone(),
                    min: min_date.clone(),
                    max: end_date.clone(),
                    oninput: move |value| {
                        start_override.set(value);
                        range_preset.set(RangePreset::Custom);
                    }
                }
                DateInput {
                    label: "End",
                    value: end_date.clone(),
                    min: start_date.clone(),
                    max: max_date.clone(),
                    oninput: move |value| {
                        end_override.set(value);
                        range_preset.set(RangePreset::Custom);
                    }
                }
                NumberInput {
                    label: "Min",
                    value: y_min_text.clone(),
                    oninput: move |value| y_min_input.set(value)
                }
                NumberInput {
                    label: "Max",
                    value: y_max_text.clone(),
                    oninput: move |value| y_max_input.set(value)
                }
            }
            if has_date_error {
                p { class: "validation", "Use a valid start date before end date." }
            }
            if has_y_error {
                p { class: "validation", "Y min must be less than Y max." }
            }
            div { class: "range-summary",
                span { "Date range {start_date} to {end_date}" }
                span { "Data range {data_y_range}" }
                span { "Axis range {axis_y_range}" }
                span { "Sampling {sampling_label} / {sampled_point_count} points" }
            }
        }
    }
}

#[component]
fn BackendActivity(message: &'static str) -> Element {
    rsx! {
        div {
            class: "backend-activity",
            role: "status",
            aria_live: "polite",
            span { class: "activity-spinner" }
            span { "{message}" }
        }
    }
}

#[component]
fn GraphLoadingPanel(range: String, sampling: &'static str) -> Element {
    rsx! {
        div {
            class: "chart-loading",
            role: "status",
            aria_live: "polite",
            span { class: "activity-spinner large" }
            strong { "Updating graph" }
            span { "{range} / {sampling}" }
        }
    }
}

#[component]
fn GraphPresetButton(
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if selected {
        "control-button selected"
    } else {
        "control-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[component]
fn DateInput(
    label: &'static str,
    value: String,
    min: String,
    max: String,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "date",
                value: "{value}",
                min: "{min}",
                max: "{max}",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
fn NumberInput(label: &'static str, value: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "number",
                value: "{value}",
                step: "0.01",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
fn NetWorthChart(
    data: Vec<NetWorthDataPoint>,
    currency: String,
    y_domain: Option<(f64, f64)>,
) -> Element {
    let values = data
        .iter()
        .map(|point| (point.date.clone(), point.value))
        .collect::<Vec<_>>();

    if values.is_empty() {
        return rsx! {
            div { class: "chart-empty",
                strong { "No net worth history" }
                small { "Sync balances to populate the chart." }
            }
        };
    }

    let width = 720.0;
    let height = 260.0;
    let padding_left = 68.0;
    let padding_right = 20.0;
    let padding_top = 18.0;
    let padding_bottom = 38.0;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;

    let min_value = values
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::INFINITY, f64::min);
    let max_value = values
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    let (y_min, y_max) = if let Some((min, max)) = y_domain {
        (min, max)
    } else {
        let range = (max_value - min_value).abs();
        let padding = if range == 0.0 {
            (max_value.abs() * 0.05).max(1.0)
        } else {
            range * 0.08
        };
        (min_value - padding, max_value + padding)
    };
    let y_range = (y_max - y_min).max(1.0);
    let count = values.len();

    let chart_points = values
        .iter()
        .enumerate()
        .map(|(index, (date, value))| {
            let x = if count <= 1 {
                padding_left + plot_width / 2.0
            } else {
                padding_left + (index as f64 / (count - 1) as f64) * plot_width
            };
            let y = padding_top + ((y_max - value) / y_range) * plot_height;
            ChartPoint {
                date: date.clone(),
                value: *value,
                x,
                y,
            }
        })
        .collect::<Vec<_>>();

    let line_path = chart_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let command = if index == 0 { "M" } else { "L" };
            format!("{command} {:.2} {:.2}", point.x, point.y)
        })
        .collect::<Vec<_>>()
        .join(" ");
    let area_path = match (chart_points.first(), chart_points.last()) {
        (Some(first), Some(last)) => format!(
            "{line_path} L {:.2} {:.2} L {:.2} {:.2} Z",
            last.x,
            padding_top + plot_height,
            first.x,
            padding_top + plot_height
        ),
        _ => String::new(),
    };
    let hover_points = chart_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let previous_x = if index == 0 {
                padding_left
            } else {
                (chart_points[index - 1].x + point.x) / 2.0
            };
            let next_x = if index + 1 == chart_points.len() {
                width - padding_right
            } else {
                (point.x + chart_points[index + 1].x) / 2.0
            };
            ChartHoverPoint {
                index,
                point: point.clone(),
                hit_x: previous_x,
                hit_width: (next_x - previous_x).max(1.0),
            }
        })
        .collect::<Vec<_>>();
    let hover_rules = hover_points
        .iter()
        .map(|hover_point| {
            format!(
                ".chart-hit-zone-{0}:hover ~ .chart-hover-detail-{0} {{ display: block; }}",
                hover_point.index
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let latest = chart_points.last().expect("values is non-empty");
    let first = chart_points.first().expect("values is non-empty");
    let y_mid = y_min + y_range / 2.0;
    let latest_value = format_compact_money(latest.value, &currency);
    let min_label = format_compact_money(y_min, &currency);
    let mid_label = format_compact_money(y_mid, &currency);
    let max_label = format_compact_money(y_max, &currency);
    let first_date = first.date.clone();
    let latest_date = latest.date.clone();
    let absolute_change = latest.value - first.value;
    let percentage_change = if first.value == 0.0 {
        None
    } else {
        Some((absolute_change / first.value) * 100.0)
    };
    let summary = percentage_change
        .map(|percentage| {
            format!(
                "{} ({}%)",
                format_signed_money(absolute_change, &currency),
                format_number(percentage, 2)
            )
        })
        .unwrap_or_else(|| "No range change".to_string());

    rsx! {
        div { class: "chart-card",
            div { class: "chart-meta",
                div {
                    span { class: "metric-label", "Current" }
                    strong { "{latest_value}" }
                }
                div {
                    span { class: "metric-label", "Range change" }
                    strong { "{summary}" }
                }
            }
            svg {
                class: "net-worth-chart",
                view_box: "0 0 720 260",
                role: "img",
                style { "{hover_rules}" }
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top}",
                    y2: "{padding_top}"
                }
                line {
                    class: "chart-grid",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height / 2.0}",
                    y2: "{padding_top + plot_height / 2.0}"
                }
                line {
                    class: "chart-grid axis",
                    x1: "{padding_left}",
                    x2: "{width - padding_right}",
                    y1: "{padding_top + plot_height}",
                    y2: "{padding_top + plot_height}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + 4.0}",
                    "{max_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height / 2.0 + 4.0}",
                    "{mid_label}"
                }
                text {
                    class: "chart-axis-label",
                    x: "8",
                    y: "{padding_top + plot_height + 4.0}",
                    "{min_label}"
                }
                text {
                    class: "chart-axis-label date-label",
                    x: "{padding_left}",
                    y: "{height - 10.0}",
                    "{first_date}"
                }
                text {
                    class: "chart-axis-label date-label end",
                    x: "{width - padding_right}",
                    y: "{height - 10.0}",
                    "{latest_date}"
                }
                if chart_points.len() > 1 {
                    path { class: "chart-area", d: "{area_path}" }
                    path { class: "chart-line", d: "{line_path}" }
                }
                for point in chart_points {
                    circle {
                        class: "chart-point",
                        cx: "{point.x}",
                        cy: "{point.y}",
                        r: "3.4",
                        title { "{point.date}: {format_full_money(point.value, &currency)}" }
                    }
                }
                g { class: "chart-hover-layer",
                    for hover_point in hover_points.iter() {
                        rect {
                            class: "chart-hit-zone chart-hit-zone-{hover_point.index}",
                            x: "{hover_point.hit_x}",
                            y: "{padding_top}",
                            width: "{hover_point.hit_width}",
                            height: "{plot_height}"
                        }
                    }
                    for hover_point in hover_points {
                        ChartHoverDetail {
                            index: hover_point.index,
                            point: hover_point.point.clone(),
                            currency: currency.clone(),
                            chart_width: width,
                            padding_right,
                            padding_top
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChartHoverDetail(
    index: usize,
    point: ChartPoint,
    currency: String,
    chart_width: f64,
    padding_right: f64,
    padding_top: f64,
) -> Element {
    let tooltip_width = 184.0;
    let tooltip_height = 50.0;
    let tooltip_x = if point.x + tooltip_width + 12.0 > chart_width - padding_right {
        point.x - tooltip_width - 12.0
    } else {
        point.x + 12.0
    }
    .max(8.0);
    let tooltip_y = if point.y - tooltip_height - 10.0 < padding_top {
        point.y + 12.0
    } else {
        point.y - tooltip_height - 10.0
    }
    .max(8.0);
    let date_y = tooltip_y + 20.0;
    let value_y = tooltip_y + 38.0;
    let text_x = tooltip_x + 10.0;
    let value_text = format_full_money(point.value, &currency);

    rsx! {
        g { class: "chart-hover-detail chart-hover-detail-{index}",
            line {
                class: "chart-hover-line",
                x1: "{point.x}",
                x2: "{point.x}",
                y1: "{padding_top}",
                y2: "{point.y}"
            }
            circle {
                class: "chart-hover-point",
                cx: "{point.x}",
                cy: "{point.y}",
                r: "6"
            }
            rect {
                class: "chart-tooltip",
                x: "{tooltip_x}",
                y: "{tooltip_y}",
                width: "{tooltip_width}",
                height: "{tooltip_height}",
                rx: "6"
            }
            text {
                class: "chart-tooltip-date",
                x: "{text_x}",
                y: "{date_y}",
                "{point.date}"
            }
            text {
                class: "chart-tooltip-value",
                x: "{text_x}",
                y: "{value_y}",
                "{value_text}"
            }
        }
    }
}

#[component]
fn TextInput(
    label: &'static str,
    value: String,
    placeholder: &'static str,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "control-field",
            span { "{label}" }
            input {
                class: "control-input",
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder}",
                oninput: move |event| oninput.call(event.value())
            }
        }
    }
}

#[component]
fn DataDirectoryControl(
    value: String,
    recommended: Option<String>,
    disabled: bool,
    onselect: EventHandler<String>,
) -> Element {
    let display_value = if value.trim().is_empty() {
        recommended
            .clone()
            .unwrap_or_else(|| "/path/to/keepbook-data".to_string())
    } else {
        value
    };

    rsx! {
        div { class: "control-field directory-field",
            span { "Data directory" }
            if let Some(path) = recommended {
                div { class: "directory-picker",
                    code { class: "directory-picker-path", "{display_value}" }
                    button {
                        class: "control-button",
                        disabled,
                        onclick: move |_| onselect.call(path.clone()),
                        "Use app data folder"
                    }
                }
            } else {
                input {
                    class: "control-input",
                    r#type: "text",
                    value: "{display_value}",
                    placeholder: "/path/to/keepbook-data",
                    disabled,
                    oninput: move |event| onselect.call(event.value())
                }
            }
        }
    }
}

#[component]
fn AccountsView(
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    balances: Vec<Balance>,
    snapshot: PortfolioSnapshot,
    currency: String,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut price_busy = use_signal(|| false);
    let mut force_prices = use_signal(|| false);
    let mut price_status = use_signal(String::new);
    let virtual_accounts = virtual_account_summaries(&snapshot);
    let account_count = accounts.len() + virtual_accounts.len();
    let account_summaries = snapshot.by_account.clone();
    let _ = balances;
    let is_price_busy = price_busy();
    let price_status_text = price_status();

    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Accounts" }
                    span { "{account_count}" }
                }
                div { class: "settings-actions inline-actions",
                    label { class: "compact-check",
                        input {
                            r#type: "checkbox",
                            checked: force_prices(),
                            disabled: is_price_busy,
                            onchange: move |event| force_prices.set(event.checked())
                        }
                        span { "Force prices" }
                    }
                    button {
                        class: "control-button",
                        disabled: is_price_busy,
                        onclick: move |_| {
                            price_busy.set(true);
                            let input = SyncPricesInput {
                                scope: "all".to_string(),
                                target: None,
                                force: force_prices(),
                                quote_staleness_seconds: None,
                            };
                            price_status.set(if input.force {
                                "Refreshing all prices...".to_string()
                            } else {
                                "Refreshing stale prices...".to_string()
                            });
                            spawn(async move {
                                match sync_prices(input).await {
                                    Ok(result) => {
                                        price_status.set(price_sync_result_summary(&result));
                                        onrefresh.call(());
                                    }
                                    Err(error) => {
                                        price_status.set(format!("Price sync failed: {error}"));
                                    }
                                }
                                price_busy.set(false);
                            });
                        },
                        if is_price_busy { "Refreshing" } else { "Sync prices" }
                    }
                }
            }
            if !price_status_text.is_empty() {
                p { class: "settings-status", "{price_status_text}" }
            }
            div { class: "group-list",
                if !virtual_accounts.is_empty() {
                    VirtualAccountGroup {
                        accounts: virtual_accounts,
                        currency: currency.clone(),
                    }
                }
                for connection in connections {
                    AccountGroup {
                        connection: connection.clone(),
                        accounts: accounts
                            .iter()
                            .filter(|account| account.connection_id == connection.id)
                            .cloned()
                            .collect::<Vec<_>>(),
                        account_summaries: account_summaries.clone(),
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountGroup(accounts: Vec<AccountSummary>, currency: String) -> Element {
    rsx! {
        section { class: "tree-group virtual-group",
            div { class: "tree-parent",
                div {
                    strong { "Virtual" }
                    small { "Portfolio adjustments" }
                }
                span { class: "status liability-status", "{accounts.len()} active" }
            }
            div { class: "data-table account-table",
                div { class: "table-head",
                    span { "Account" }
                    span { "Balance ({currency})" }
                    span { "Status" }
                    span { "Tags" }
                }
                for account in accounts {
                    VirtualAccountRow {
                        account,
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn VirtualAccountRow(account: AccountSummary, currency: String) -> Element {
    let value = account
        .value_in_base
        .as_deref()
        .and_then(parse_money_input)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "table-row virtual-account-row",
            strong { "{account.account_name}" }
            span { "{value}" }
            span { class: "status liability-status", "Virtual" }
            small { "{account.connection_name}" }
        }
    }
}

#[component]
fn AccountGroup(
    connection: Connection,
    accounts: Vec<Account>,
    account_summaries: Vec<AccountSummary>,
    currency: String,
) -> Element {
    let active_count = accounts.iter().filter(|account| account.active).count();
    let ignored_count = accounts
        .iter()
        .filter(|account| account.exclude_from_portfolio)
        .count();
    let status_text = if ignored_count == 0 {
        format!("{active_count}/{} active", accounts.len())
    } else {
        format!(
            "{active_count}/{} active, {ignored_count} ignored",
            accounts.len()
        )
    };

    rsx! {
        section { class: "tree-group",
            div { class: "tree-parent",
                div {
                    strong { "{connection.name}" }
                    small { "{connection.synchronizer}" }
                }
                span { class: "status", "{status_text}" }
            }
            div { class: "data-table account-table",
                div { class: "table-head",
                    span { "Account" }
                    span { "Balance ({currency})" }
                    span { "Status" }
                    span { "Tags" }
                }
                for account in accounts {
                    AccountRow {
                        account,
                        account_summaries: account_summaries.clone(),
                        currency: currency.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn AccountRow(
    account: Account,
    account_summaries: Vec<AccountSummary>,
    currency: String,
) -> Element {
    let status = if account.exclude_from_portfolio {
        "Ignored"
    } else if account.active {
        "Active"
    } else {
        "Inactive"
    };
    let row_class = if account.exclude_from_portfolio {
        "table-row ignored-account-row"
    } else {
        "table-row"
    };
    let status_class = if account.exclude_from_portfolio {
        "status ignored-status"
    } else {
        "status"
    };
    let tags = account.tags.join(", ");
    let balance = account_snapshot_value(&account.id, &account_summaries)
        .map(|value| format_full_money(value, &currency))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "{row_class}",
            strong { "{account.name}" }
            span { "{balance}" }
            span { class: "{status_class}", "{status}" }
            small { "{tags}" }
        }
    }
}

#[component]
fn ConnectionsView(connections: Vec<Connection>) -> Element {
    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Connections" }
                span { "{connections.len()}" }
            }
            div { class: "data-table connection-table",
                div { class: "table-head",
                    span { "Name" }
                    span { "Sync" }
                    span { "Accounts" }
                    span { "Last sync" }
                }
                for connection in connections {
                    div { class: "table-row",
                        strong { "{connection.name}" }
                        span { class: "status", "{connection.status}" }
                        span { "{connection.account_count}" }
                        small {
                            "{connection.last_sync.clone().unwrap_or_else(|| \"Never\".to_string())}"
                        }
                    }
                }
            }
        }
    }
}

fn price_sync_result_summary(result: &serde_json::Value) -> String {
    let Some(refresh) = result.get("result") else {
        return "Price sync finished.".to_string();
    };
    let fetched = refresh
        .get("fetched")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let skipped = refresh
        .get("skipped")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let failed = refresh
        .get("failed_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if failed == 0 {
        format!("Price sync complete: {fetched} fetched, {skipped} skipped.")
    } else {
        format!("Price sync complete: {fetched} fetched, {skipped} skipped, {failed} failed.")
    }
}

#[component]
fn BalancesView(balances: Vec<Balance>) -> Element {
    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Balances" }
                span { "{balances.len()}" }
            }
            div { class: "data-table balance-table",
                div { class: "table-head",
                    span { "Asset" }
                    span { "Amount" }
                    span { "Account" }
                    span { "Value" }
                    span { "Timestamp" }
                }
                for balance in balances {
                    div { class: "table-row",
                        strong { "{asset_label(&balance.asset)}" }
                        span { "{balance.amount}" }
                        small { "{balance.account_id}" }
                        span {
                            "{balance.value_in_reporting_currency.clone().unwrap_or_else(|| \"N/A\".to_string())} {balance.reporting_currency}"
                        }
                        small { "{balance.timestamp}" }
                    }
                }
            }
        }
    }
}

#[component]
fn ProposedEditsView(onrefresh: EventHandler<()>) -> Element {
    let mut proposals = use_resource(fetch_proposed_transaction_edits);
    let mut busy_id = use_signal(String::new);
    let mut status = use_signal(String::new);
    let current = proposals.cloned();
    let busy = busy_id();
    let status_text = status();

    rsx! {
        section { class: "panel",
            div { class: "panel-header",
                h2 { "Proposed transaction edits" }
                button {
                    class: "control-button",
                    disabled: !busy.is_empty(),
                    onclick: move |_| proposals.restart(),
                    "Refresh"
                }
            }
            if !status_text.is_empty() {
                p { class: "settings-status", "{status_text}" }
            }
            match current {
                None => rsx! { BackendActivity { message: "Loading proposed edits" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(items)) => rsx! {
                    if items.is_empty() {
                        div { class: "chart-empty proposal-empty",
                            strong { "No pending edits" }
                            small { "Approved, rejected, and removed edits are hidden from this queue." }
                        }
                    } else {
                        div { class: "data-table proposed-edits-table",
                            div { class: "table-head",
                                span { "Transaction" }
                                span { "Account" }
                                span { "Patch" }
                                span { "Created" }
                                span { "Actions" }
                            }
                            for edit in items {
                                ProposedEditRow {
                                    edit: edit.clone(),
                                    busy: busy.clone(),
                                    ondecide: move |(id, action): (String, &'static str)| {
                                        busy_id.set(id.clone());
                                        status.set(format!("{action} {id}..."));
                                        spawn(async move {
                                            match decide_proposed_transaction_edit(id.clone(), action).await {
                                                Ok(()) => {
                                                    status.set(format!("{} {id}.", proposal_action_past_tense(action)));
                                                    proposals.restart();
                                                    onrefresh.call(());
                                                }
                                                Err(error) => status.set(error),
                                            }
                                            busy_id.set(String::new());
                                        });
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ProposedEditRow(
    edit: ProposedTransactionEdit,
    busy: String,
    ondecide: EventHandler<(String, &'static str)>,
) -> Element {
    let is_busy = busy == edit.id;
    let any_busy = !busy.is_empty();
    let patch = proposed_patch_summary(&edit.patch);
    let amount_class = if edit.transaction_amount.trim_start().starts_with('-') {
        "change-negative"
    } else {
        "change-positive"
    };
    let approve_id = edit.id.clone();
    let reject_id = edit.id.clone();
    let remove_id = edit.id.clone();

    rsx! {
        div { class: "table-row",
            div { class: "proposal-transaction-cell",
                strong { "{edit.transaction_description}" }
                small { "{edit.transaction_timestamp}" }
                small { class: "{amount_class}", "{edit.transaction_amount}" }
            }
            small { "{edit.account_name}" }
            small { "{patch}" }
            small { "{edit.created_at}" }
            div { class: "proposal-actions",
                button {
                    class: "control-button selected",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((approve_id.clone(), "approve")),
                    if is_busy { "Working" } else { "Approve" }
                }
                button {
                    class: "control-button",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((reject_id.clone(), "reject")),
                    "Reject"
                }
                button {
                    class: "control-button danger-button",
                    disabled: any_busy,
                    onclick: move |_| ondecide.call((remove_id.clone(), "remove")),
                    "Remove"
                }
            }
        }
    }
}

#[component]
fn HistoryView(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    let initial_range_preset = range_preset_from_config(&defaults.graph_range);
    let initial_sampling_granularity =
        sampling_granularity_from_config(&defaults.graph_granularity);
    let history = use_resource(move || async move {
        fetch_history(history_query_string(
            initial_range_preset,
            "",
            "",
            initial_sampling_granularity,
            &current_date_string(),
            filter_overrides,
        ))
        .await
    });
    rsx! {
        section { class: "panel",
            match history.cloned() {
                None => rsx! { InlineStatus { title: "Net Worth History", message: "Loading history..." } },
                Some(Ok(history)) => rsx! {
                    HistoryTable { history, currency }
                },
                Some(Err(error)) => rsx! {
                    InlineStatus { title: "Net Worth History", message: error }
                },
            }
        }
    }
}

#[component]
fn HistoryTable(history: History, currency: String) -> Element {
    let row_count = history.points.len();

    rsx! {
        div { class: "panel-header",
            h2 { "Net Worth History" }
            span { "{row_count}" }
        }
        div { class: "data-table history-table",
            div { class: "table-head",
                span { "Date" }
                span { "Net worth" }
                span { "Daily change" }
            }
            for point in history.points.iter().rev() {
                HistoryPointRow {
                    point: point.clone(),
                    currency: currency.clone()
                }
            }
        }
    }
}

#[component]
fn HistoryPointRow(point: HistoryPoint, currency: String) -> Element {
    let total = point.total_value.parse::<f64>().unwrap_or_default();
    let total_text = format_full_money(total, &currency);
    let change_text = point
        .percentage_change_from_previous
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "N/A".to_string());

    rsx! {
        div { class: "table-row",
            strong { "{point.date}" }
            span { "{total_text}" }
            small { "{change_text}" }
        }
    }
}

fn asset_label(asset: &serde_json::Value) -> String {
    let Some(obj) = asset.as_object() else {
        return asset
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| asset.to_string());
    };

    match obj.get("type").and_then(|value| value.as_str()) {
        Some("currency") => obj
            .get("iso_code")
            .or_else(|| obj.get("currency"))
            .and_then(|value| value.as_str())
            .map(normalize_currency_code_for_display)
            .unwrap_or_else(|| "Currency".to_string()),
        Some("equity") => {
            let ticker = obj
                .get("ticker")
                .or_else(|| obj.get("symbol"))
                .and_then(|value| value.as_str())
                .unwrap_or("Equity");
            match obj.get("exchange").and_then(|value| value.as_str()) {
                Some(exchange) if !exchange.trim().is_empty() => {
                    format!("{} ({})", ticker.trim(), exchange.trim())
                }
                _ => ticker.trim().to_string(),
            }
        }
        Some("crypto") => {
            let symbol = obj
                .get("symbol")
                .and_then(|value| value.as_str())
                .unwrap_or("Crypto");
            match obj.get("network").and_then(|value| value.as_str()) {
                Some(network) if !network.trim().is_empty() => {
                    format!("{} ({})", symbol.trim(), network.trim())
                }
                _ => symbol.trim().to_string(),
            }
        }
        _ => obj
            .get("symbol")
            .and_then(|value| value.as_str())
            .or_else(|| obj.get("ticker").and_then(|value| value.as_str()))
            .or_else(|| obj.get("iso_code").and_then(|value| value.as_str()))
            .or_else(|| obj.get("currency").and_then(|value| value.as_str()))
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| asset.to_string()),
    }
}

fn normalize_currency_code_for_display(value: &str) -> String {
    match value.trim() {
        "840" => "USD".to_string(),
        trimmed => trimmed.to_uppercase(),
    }
}

fn proposed_patch_summary(patch: &ProposedTransactionEditPatch) -> String {
    let mut parts = Vec::new();
    push_patch_part(&mut parts, "description", &patch.description);
    push_patch_part(&mut parts, "note", &patch.note);
    push_patch_part(&mut parts, "category", &patch.category);
    if let Some(value) = &patch.tags {
        match value {
            Some(tags) => parts.push(format!("tags={}", tags.join(", "))),
            None => parts.push("tags=clear".to_string()),
        }
    }
    push_patch_part(&mut parts, "effective_date", &patch.effective_date);
    if parts.is_empty() {
        "No changes".to_string()
    } else {
        parts.join("; ")
    }
}

fn push_patch_part(parts: &mut Vec<String>, label: &str, value: &Option<Option<String>>) {
    if let Some(value) = value {
        match value {
            Some(text) => parts.push(format!("{label}={text}")),
            None => parts.push(format!("{label}=clear")),
        }
    }
}

fn proposal_action_past_tense(action: &str) -> &'static str {
    match action {
        "approve" => "Approved",
        "reject" => "Rejected",
        "remove" => "Removed",
        _ => "Updated",
    }
}

fn current_net_worth_from_snapshot(snapshot: &PortfolioSnapshot) -> f64 {
    snapshot
        .total_value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or_default()
}

fn history_data_points(history: &History) -> Vec<NetWorthDataPoint> {
    let mut points = history
        .points
        .iter()
        .filter_map(|point| {
            point
                .total_value
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| NetWorthDataPoint {
                    date: point.date.clone(),
                    value,
                })
        })
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

fn date_bounds(points: &[NetWorthDataPoint]) -> Option<(String, String)> {
    Some((points.first()?.date.clone(), points.last()?.date.clone()))
}

fn visible_date_range(
    points: &[NetWorthDataPoint],
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
) -> (String, String) {
    let Some((min_date, max_date)) = date_bounds(points) else {
        return (String::new(), String::new());
    };

    if preset == RangePreset::Custom {
        return (
            if start_override.is_empty() {
                min_date.clone()
            } else {
                start_override.to_string()
            },
            if end_override.is_empty() {
                max_date.clone()
            } else {
                end_override.to_string()
            },
        );
    }

    let end = max_date.clone();
    let start = match preset {
        RangePreset::OneMonth => offset_months(&end, 1).max(min_date.clone()),
        RangePreset::NinetyDays => offset_days(&end, 90).max(min_date.clone()),
        RangePreset::SixMonths => offset_months(&end, 6).max(min_date.clone()),
        RangePreset::OneYear => offset_years(&end, 1).max(min_date.clone()),
        RangePreset::TwoYears => offset_years(&end, 2).max(min_date.clone()),
        RangePreset::Max | RangePreset::Custom => min_date.clone(),
    };
    (start, end)
}

fn history_query_string(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    selected_sampling: SamplingGranularity,
    today: &str,
    filter_overrides: FilterOverrides,
) -> String {
    let (start, end) = requested_history_date_range(preset, start_override, end_override, today);
    let granularity =
        history_request_granularity(selected_sampling, start.as_deref(), end.as_deref());
    let mut params = vec![format!("granularity={granularity}")];

    if let Some(start) = start {
        params.push(format!("start={start}"));
    }
    if let Some(end) = end {
        params.push(format!("end={end}"));
    }
    append_filter_override_params(&mut params, filter_overrides);

    params.join("&")
}

#[cfg(any(target_arch = "wasm32", test))]
fn filter_override_query_string(overrides: FilterOverrides) -> String {
    let mut params = Vec::new();
    append_filter_override_params(&mut params, overrides);
    params.join("&")
}

fn append_filter_override_params(params: &mut Vec<String>, overrides: FilterOverrides) {
    if let Some(enabled) = overrides.include_latent_capital_gains_tax {
        params.push(format!(
            "include_latent_capital_gains_tax={}",
            bool_query_value(enabled)
        ));
    }
}

fn bool_query_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn requested_history_date_range(
    preset: RangePreset,
    start_override: &str,
    end_override: &str,
    today: &str,
) -> (Option<String>, Option<String>) {
    if preset == RangePreset::Custom {
        return (
            non_empty_string(start_override),
            non_empty_string(end_override).or_else(|| Some(today.to_string())),
        );
    }

    let end = Some(today.to_string());
    let start = match preset {
        RangePreset::OneMonth => Some(offset_months(today, 1)),
        RangePreset::NinetyDays => Some(offset_days(today, 90)),
        RangePreset::SixMonths => Some(offset_months(today, 6)),
        RangePreset::OneYear => Some(offset_years(today, 1)),
        RangePreset::TwoYears => Some(offset_years(today, 2)),
        RangePreset::Max | RangePreset::Custom => None,
    };

    if preset == RangePreset::Max {
        (None, None)
    } else {
        (start, end)
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn range_summary_text(start: &str, end: &str) -> String {
    match (start.is_empty(), end.is_empty()) {
        (false, false) => format!("{start} to {end}"),
        (false, true) => format!("{start} onward"),
        (true, false) => format!("through {end}"),
        (true, true) => "All available dates".to_string(),
    }
}

fn history_request_granularity(
    selected: SamplingGranularity,
    start: Option<&str>,
    end: Option<&str>,
) -> &'static str {
    if selected != SamplingGranularity::Auto {
        return selected.query_value();
    }

    match (start, end) {
        (Some(start), Some(end)) => match days_between(start, end) {
            Some(days) if days < 93 => SamplingGranularity::Daily.query_value(),
            Some(days) if days > 365 * 3 => SamplingGranularity::Monthly.query_value(),
            Some(_) => SamplingGranularity::Weekly.query_value(),
            None => SamplingGranularity::Daily.query_value(),
        },
        _ => SamplingGranularity::Monthly.query_value(),
    }
}

#[cfg(target_arch = "wasm32")]
fn current_date_string() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date()
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn current_date_string() -> String {
    chrono::Local::now().date_naive().to_string()
}

fn offset_years(date: &str, years: i32) -> String {
    offset_months(date, years * 12)
}

fn offset_months(date: &str, months: i32) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };

    let month_index = year * 12 + month as i32 - 1 - months;
    let new_year = month_index.div_euclid(12);
    let new_month = month_index.rem_euclid(12) as u32 + 1;
    let new_day = day.min(days_in_month(new_year, new_month));
    format!("{new_year:04}-{new_month:02}-{new_day:02}")
}

fn offset_days(date: &str, days: i64) -> String {
    let Some((year, month, day)) = parse_ymd(date) else {
        return date.to_string();
    };
    civil_from_days(days_from_civil(year, month, day) - days)
}

fn filter_data_by_date_range(
    points: &[NetWorthDataPoint],
    start_date: &str,
    end_date: &str,
) -> Vec<NetWorthDataPoint> {
    if start_date.is_empty() || end_date.is_empty() || start_date > end_date {
        return Vec::new();
    }

    points
        .iter()
        .filter(|point| point.date.as_str() >= start_date && point.date.as_str() <= end_date)
        .cloned()
        .collect()
}

fn resolve_sampling_granularity(
    selected: SamplingGranularity,
    points: &[NetWorthDataPoint],
) -> SamplingGranularity {
    if selected != SamplingGranularity::Auto {
        return selected;
    }

    let Some(first) = points.first() else {
        return SamplingGranularity::Daily;
    };
    let Some(last) = points.last() else {
        return SamplingGranularity::Daily;
    };

    match days_between(&first.date, &last.date) {
        Some(days) if days < 93 => SamplingGranularity::Daily,
        Some(days) if days > 365 * 3 => SamplingGranularity::Monthly,
        Some(_) => SamplingGranularity::Weekly,
        _ => SamplingGranularity::Daily,
    }
}

fn sample_data_by_granularity(
    points: &[NetWorthDataPoint],
    granularity: SamplingGranularity,
) -> Vec<NetWorthDataPoint> {
    if matches!(
        granularity,
        SamplingGranularity::Auto | SamplingGranularity::Daily
    ) || points.len() <= 2
    {
        return points.to_vec();
    }

    let mut sampled = Vec::new();
    let mut current_bucket: Option<String> = None;
    let mut current_point: Option<NetWorthDataPoint> = None;

    for point in points {
        let bucket = sampling_bucket(&point.date, granularity);
        if current_bucket.as_deref() != Some(bucket.as_str()) {
            if let Some(point) = current_point.take() {
                sampled.push(point);
            }
            current_bucket = Some(bucket);
        }
        current_point = Some(point.clone());
    }

    if let Some(point) = current_point {
        sampled.push(point);
    }

    include_range_endpoints(points, sampled)
}

fn include_range_endpoints(
    points: &[NetWorthDataPoint],
    sampled: Vec<NetWorthDataPoint>,
) -> Vec<NetWorthDataPoint> {
    let Some(first) = points.first() else {
        return sampled;
    };
    let Some(last) = points.last() else {
        return sampled;
    };

    let mut with_endpoints = sampled;
    if !with_endpoints.iter().any(|point| point.date == first.date) {
        with_endpoints.push(first.clone());
    }
    if !with_endpoints.iter().any(|point| point.date == last.date) {
        with_endpoints.push(last.clone());
    }
    with_endpoints.sort_by(|a, b| a.date.cmp(&b.date));
    with_endpoints
}

fn sampling_bucket(date: &str, granularity: SamplingGranularity) -> String {
    match granularity {
        SamplingGranularity::Weekly => parse_ymd(date)
            .map(|(year, month, day)| {
                let day_number = days_from_civil(year, month, day);
                format!("week-{}", day_number.div_euclid(7))
            })
            .unwrap_or_else(|| date.to_string()),
        SamplingGranularity::Monthly => date.get(..7).unwrap_or(date).to_string(),
        SamplingGranularity::Yearly => date.get(..4).unwrap_or(date).to_string(),
        SamplingGranularity::Auto | SamplingGranularity::Daily => date.to_string(),
    }
}

fn days_between(start: &str, end: &str) -> Option<i64> {
    let (start_year, start_month, start_day) = parse_ymd(start)?;
    let (end_year, end_month, end_day) = parse_ymd(end)?;
    Some(
        days_from_civil(end_year, end_month, end_day)
            - days_from_civil(start_year, start_month, start_day),
    )
}

fn parse_ymd(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = (year as i64).div_euclid(400);
    let yoe = year as i64 - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

fn value_bounds(points: &[NetWorthDataPoint]) -> Option<(f64, f64)> {
    let first = points.first()?.value;
    let mut min = first;
    let mut max = first;
    for point in points {
        min = min.min(point.value);
        max = max.max(point.value);
    }
    Some(if min == max {
        (min - 1.0, max + 1.0)
    } else {
        (min, max)
    })
}

fn parse_money_input(value: &str) -> Option<f64> {
    let cleaned = value
        .chars()
        .filter(|ch| !matches!(ch, '$' | ',' | ' '))
        .collect::<String>();
    if cleaned.is_empty() {
        None
    } else {
        cleaned
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

fn account_snapshot_value(account_id: &str, account_summaries: &[AccountSummary]) -> Option<f64> {
    account_summaries
        .iter()
        .find(|summary| summary.account_id == account_id)
        .and_then(|summary| summary.value_in_base.as_deref())
        .and_then(parse_money_input)
}

fn virtual_account_summaries(snapshot: &PortfolioSnapshot) -> Vec<AccountSummary> {
    snapshot
        .by_account
        .iter()
        .filter(|account| account.account_id.starts_with("virtual:"))
        .cloned()
        .collect()
}

fn parse_y_domain(min: &str, max: &str) -> Option<(f64, f64)> {
    if min.is_empty() && max.is_empty() {
        return None;
    }
    let min = parse_money_input(min)?;
    let max = parse_money_input(max)?;
    if min < max {
        Some((min, max))
    } else {
        None
    }
}

fn format_input_number(value: f64) -> String {
    format_number(value, 2)
}

fn format_compact_money(value: f64, currency: &str) -> String {
    let abs = value.abs();
    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (value / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (value / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (value / 1_000.0, "K")
    } else {
        (value, "")
    };
    format!("{currency} {}{suffix}", format_number(scaled, 1))
}

fn format_full_money(value: f64, currency: &str) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let abs = value.abs();
    let integer = abs.trunc() as i64;
    let decimals = ((abs.fract() * 100.0).round() as i64).min(99);
    let prefix = if currency.is_empty() {
        String::new()
    } else {
        format!("{currency} ")
    };
    format!(
        "{prefix}{sign}{}.{:02}",
        format_integer_with_commas(integer),
        decimals
    )
}

fn format_signed_money(value: f64, currency: &str) -> String {
    if value >= 0.0 {
        format!("+{}", format_full_money(value, currency))
    } else {
        format_full_money(value, currency)
    }
}

fn format_integer_with_commas(value: i64) -> String {
    let digits = value.to_string();
    let mut formatted = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

fn format_number(value: f64, decimals: usize) -> String {
    let mut formatted = format!("{value:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    formatted
}

fn enabled_label(value: bool) -> &'static str {
    if value {
        "Included"
    } else {
        "Excluded"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(date: &str, value: f64) -> NetWorthDataPoint {
        NetWorthDataPoint {
            date: date.to_string(),
            value,
        }
    }

    #[test]
    fn two_year_range_starts_two_years_before_latest_point() {
        let points = vec![
            point("2022-01-01", 100.0),
            point("2024-04-25", 150.0),
            point("2026-04-25", 200.0),
        ];

        assert_eq!(
            visible_date_range(&points, RangePreset::TwoYears, "", ""),
            ("2024-04-25".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn two_year_range_clamps_to_earliest_available_point() {
        let points = vec![point("2026-02-03", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::TwoYears, "", ""),
            ("2026-02-03".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn custom_range_uses_manual_overrides() {
        let points = vec![point("2024-01-01", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::Custom, "2025-01-01", "2025-12-31"),
            ("2025-01-01".to_string(), "2025-12-31".to_string())
        );
    }

    #[test]
    fn short_range_presets_use_expected_start_dates() {
        let points = vec![point("2025-01-01", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            visible_date_range(&points, RangePreset::OneMonth, "", ""),
            ("2026-03-25".to_string(), "2026-04-25".to_string())
        );
        assert_eq!(
            visible_date_range(&points, RangePreset::NinetyDays, "", ""),
            ("2026-01-25".to_string(), "2026-04-25".to_string())
        );
        assert_eq!(
            visible_date_range(&points, RangePreset::SixMonths, "", ""),
            ("2025-10-25".to_string(), "2026-04-25".to_string())
        );
    }

    #[test]
    fn default_graph_query_requests_one_year_weekly_history() {
        assert_eq!(
            history_query_string(
                DEFAULT_RANGE_PRESET,
                "",
                "",
                DEFAULT_SAMPLING_GRANULARITY,
                "2026-04-25",
                FilterOverrides::default()
            ),
            "granularity=weekly&start=2025-04-25&end=2026-04-25"
        );
    }

    #[test]
    fn graph_defaults_parse_config_values() {
        assert_eq!(range_preset_from_config("2y"), RangePreset::TwoYears);
        assert_eq!(range_preset_from_config("one_month"), RangePreset::OneMonth);
        assert_eq!(
            sampling_granularity_from_config("monthly"),
            SamplingGranularity::Monthly
        );
        assert_eq!(
            sampling_granularity_from_config("not-a-real-value"),
            DEFAULT_SAMPLING_GRANULARITY
        );
    }

    #[test]
    fn auto_graph_query_uses_daily_under_three_months() {
        assert_eq!(
            history_query_string(
                RangePreset::NinetyDays,
                "",
                "",
                SamplingGranularity::Auto,
                "2026-04-25",
                FilterOverrides::default()
            ),
            "granularity=daily&start=2026-01-25&end=2026-04-25"
        );
    }

    #[test]
    fn max_graph_query_uses_monthly_without_date_bounds() {
        assert_eq!(
            history_query_string(
                RangePreset::Max,
                "",
                "",
                SamplingGranularity::Auto,
                "2026-04-25",
                FilterOverrides::default()
            ),
            "granularity=monthly"
        );
    }

    #[test]
    fn filter_override_query_includes_latent_tax_override() {
        assert_eq!(
            filter_override_query_string(FilterOverrides {
                include_latent_capital_gains_tax: Some(false),
            }),
            "include_latent_capital_gains_tax=false"
        );
    }

    #[test]
    fn asset_label_formats_tagged_assets() {
        assert_eq!(
            asset_label(&serde_json::json!({"type":"currency","iso_code":"USD"})),
            "USD"
        );
        assert_eq!(
            asset_label(&serde_json::json!({"type":"currency","iso_code":"840"})),
            "USD"
        );
        assert_eq!(
            asset_label(&serde_json::json!({"type":"equity","ticker":"AAPL"})),
            "AAPL"
        );
        assert_eq!(
            asset_label(&serde_json::json!({"type":"crypto","symbol":"ETH","network":"base"})),
            "ETH (base)"
        );
    }

    #[test]
    fn month_offsets_clamp_to_valid_dates() {
        assert_eq!(offset_months("2026-03-31", 1), "2026-02-28");
        assert_eq!(offset_months("2024-03-31", 1), "2024-02-29");
        assert_eq!(offset_years("2024-02-29", 1), "2023-02-28");
    }

    #[test]
    fn auto_sampling_uses_daily_under_three_months() {
        let points = vec![point("2026-01-26", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            resolve_sampling_granularity(SamplingGranularity::Auto, &points),
            SamplingGranularity::Daily
        );
    }

    #[test]
    fn auto_sampling_uses_weekly_for_two_year_ranges() {
        let points = vec![point("2024-04-25", 100.0), point("2026-04-25", 200.0)];

        assert_eq!(
            resolve_sampling_granularity(SamplingGranularity::Auto, &points),
            SamplingGranularity::Weekly
        );
    }

    #[test]
    fn sampled_series_preserves_range_endpoints() {
        let points = vec![
            point("2026-01-01", 100.0),
            point("2026-01-02", 110.0),
            point("2026-01-08", 120.0),
            point("2026-01-09", 130.0),
        ];

        let sampled = sample_data_by_granularity(&points, SamplingGranularity::Weekly);

        assert_eq!(
            sampled.first().map(|point| point.date.as_str()),
            Some("2026-01-01")
        );
        assert_eq!(
            sampled.last().map(|point| point.date.as_str()),
            Some("2026-01-09")
        );
    }

    #[test]
    fn current_net_worth_uses_portfolio_snapshot_total() {
        let snapshot = PortfolioSnapshot {
            as_of_date: "2026-04-25".to_string(),
            currency: "USD".to_string(),
            total_value: "1234.56".to_string(),
            by_account: Vec::new(),
        };

        assert_eq!(current_net_worth_from_snapshot(&snapshot), 1234.56);
    }

    #[test]
    fn account_value_uses_portfolio_snapshot_account_total() {
        let account_summaries = vec![AccountSummary {
            account_id: "empower".to_string(),
            account_name: "Empower Retirement".to_string(),
            connection_name: "Empower".to_string(),
            value_in_base: Some("113738.71".to_string()),
        }];

        assert_eq!(
            account_snapshot_value("empower", &account_summaries),
            Some(113738.71)
        );
        assert_eq!(account_snapshot_value("missing", &account_summaries), None);
    }

    #[test]
    fn portfolio_snapshot_deserializes_virtual_accounts() {
        let snapshot: PortfolioSnapshot = serde_json::from_value(serde_json::json!({
            "as_of_date": "2026-04-26",
            "currency": "USD",
            "total_value": "1882543.57",
            "by_account": [
                {
                    "account_id": "acct-1",
                    "account_name": "Brokerage",
                    "connection_name": "Schwab",
                    "value_in_base": "2052806.85"
                },
                {
                    "account_id": "virtual:latent_capital_gains_tax",
                    "account_name": "Latent Capital Gains Tax",
                    "connection_name": "Virtual",
                    "value_in_base": "-170263.28"
                }
            ]
        }))
        .expect("snapshot should deserialize");

        let virtual_accounts = virtual_account_summaries(&snapshot);

        assert_eq!(virtual_accounts.len(), 1);
        assert_eq!(virtual_accounts[0].account_name, "Latent Capital Gains Tax");
        assert_eq!(
            virtual_accounts[0].value_in_base.as_deref(),
            Some("-170263.28")
        );
    }
}
