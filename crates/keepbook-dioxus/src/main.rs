use serde::{Deserialize, Serialize};

mod api;
mod logic;
mod views;

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
mod tray;

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_PACKAGE_DATA_DIR: &str = "/data/user/0/org.colonelpanic.keepbook.dioxus";

const APP_CSS: &str = include_str!("../assets/styles.css");
const APP_NAME: &str = "keepbook drop diodes";
#[cfg(feature = "desktop")]
const APP_ID: &str = "org.colonelpanic.keepbook.dioxus";
#[cfg(feature = "desktop")]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/keepbook-icon-64.png");
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
const CONTEXT_MENU_COPY_BRIDGE_JS: &str = r#"
(function () {
  if (window.__keepbookContextMenuCopyBridgeInstalled) {
    return;
  }
  window.__keepbookContextMenuCopyBridgeInstalled = true;

  // Dioxus desktop suppresses the native context menu in release builds by
  // registering a bubble-phase `contextmenu` handler on `document` that calls
  // preventDefault(). That also removes the "Copy" entry, so selected text
  // (such as sync error messages) can be highlighted but not copied. Re-enable
  // the native menu whenever there is an active text selection or an editable
  // target by stopping the event during the capture phase, before Dioxus's
  // bubble-phase handler runs.
  document.addEventListener(
    "contextmenu",
    function (event) {
      var selection = window.getSelection();
      var hasSelection =
        selection && !selection.isCollapsed && selection.toString().length > 0;
      var target = event.target;
      var isEditable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target && target.isContentEditable);
      if (hasSelection || isEditable) {
        event.stopImmediatePropagation();
      }
    },
    true
  );
})();
"#;
const THEME_BOOTSTRAP_JS: &str = r#"
(function () {
  if (window.__keepbookThemeBootstrapInstalled) {
    return;
  }
  window.__keepbookThemeBootstrapInstalled = true;

  // Apply the persisted theme as early as possible so the app does not flash the
  // default "fern" palette before the Rust theme state loads. "fern" is the
  // `:root` default, so only non-fern themes need an explicit attribute.
  try {
    var stored = localStorage.getItem("keepbook-theme");
    if (stored && stored !== "fern") {
      document.documentElement.dataset.theme = stored;
    }
  } catch (error) {}
})();
"#;
#[cfg(target_arch = "wasm32")]
const API_BASE: &str = match option_env!("KEEPBOOK_API_BASE") {
    Some(value) => value,
    None => "http://127.0.0.1:8799",
};
const DEFAULT_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SAMPLING_GRANULARITY: SamplingGranularity = SamplingGranularity::Weekly;
const DEFAULT_SPENDING_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SPENDING_BUCKET: SpendingBucket = SpendingBucket::Monthly;

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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct RepositoryRegistry {
    config_path: String,
    active_repository: Option<String>,
    repositories: Vec<Repository>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Repository {
    id: String,
    name: String,
    path: String,
    remote: String,
    branch: String,
    active: bool,
    cloned: bool,
    commit: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct AddRepositoryInput {
    name: String,
    path: String,
    remote: String,
    branch: String,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FilterOverrides {
    include_latent_capital_gains_tax: Option<bool>,
    account_portfolio_exclusions: Vec<AccountPortfolioExclusionOverride>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct AccountPortfolioExclusionOverride {
    account_id: String,
    exclude_from_portfolio: bool,
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
    tags: Option<Option<Vec<String>>>,
    subtags: Option<Option<Vec<String>>>,
    effective_date: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct RecurringTransaction {
    candidate_key: String,
    review_status: String,
    name: String,
    normalized_name: String,
    status: String,
    cadence: String,
    #[serde(default)]
    estimated_interval_days: String,
    #[serde(default)]
    estimated_recurring_cost: String,
    #[serde(default)]
    estimated_annual_cost: String,
    confidence: String,
    cadence_score: String,
    occurrence_count: usize,
    first_seen: String,
    last_seen: String,
    #[serde(default)]
    next_expected: Option<String>,
    amount: RecurringTransactionAmount,
    reason_codes: Vec<String>,
    transactions: Vec<RecurringTransactionOccurrence>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct RecurringTransactionAmount {
    typical: String,
    min: String,
    max: String,
    asset: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct RecurringTransactionOccurrence {
    id: String,
    account_id: String,
    account_name: String,
    date: String,
    description: String,
    amount: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct RecurringTransactionReviewInput {
    status: String,
    candidate: RecurringTransaction,
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
struct StackedHistory {
    currency: String,
    points: Vec<StackedHistoryPoint>,
    series: Vec<StackedHistorySeries>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct StackedHistoryPoint {
    date: String,
    total_value: String,
    components: Vec<StackedHistoryComponent>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct StackedHistoryComponent {
    series_key: String,
    value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct StackedHistorySeries {
    key: String,
    label: String,
    series_type: String,
    account_id: Option<String>,
    account_name: Option<String>,
    connection_name: Option<String>,
    parent_key: Option<String>,
    asset: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AssetBreakdown {
    as_of_date: String,
    currency: String,
    #[serde(default)]
    change_mode: String,
    total_value: String,
    assets: Vec<AssetBreakdownEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AssetBreakdownEntry {
    asset: serde_json::Value,
    asset_id: String,
    liability: bool,
    total_amount: String,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    price_date: Option<String>,
    #[serde(default)]
    price_updated_at: Option<String>,
    #[serde(default)]
    amount_last_checked_at: Option<String>,
    #[serde(default)]
    amount_last_changed_at: Option<String>,
    #[serde(default)]
    value_in_base: Option<String>,
    changes: AssetChanges,
    holdings: Vec<AssetBreakdownHolding>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
struct AssetChanges {
    #[serde(default)]
    day: Option<AssetChange>,
    #[serde(default)]
    week: Option<AssetChange>,
    #[serde(default)]
    month: Option<AssetChange>,
    #[serde(default)]
    year: Option<AssetChange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AssetChange {
    absolute: String,
    #[serde(default)]
    percentage: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AssetBreakdownHolding {
    account_id: String,
    account_name: String,
    #[serde(default)]
    connection_name: Option<String>,
    amount: String,
    balance_date: String,
    #[serde(default)]
    value_in_base: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SpendingOutput {
    currency: String,
    tz: String,
    start_date: String,
    end_date: String,
    #[serde(default)]
    period: String,
    total: String,
    transaction_count: usize,
    periods: Vec<SpendingPeriod>,
    skipped_transaction_count: usize,
    missing_price_transaction_count: usize,
    missing_fx_transaction_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SpendingPeriod {
    start_date: String,
    end_date: String,
    total: String,
    transaction_count: usize,
    #[serde(default)]
    breakdown: Vec<SpendingBreakdownEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SpendingBreakdownEntry {
    key: String,
    total: String,
    transaction_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SpendingDashboardData {
    spending: SpendingOutput,
    spending_over_time: SpendingOutput,
    exact_match_spending: SpendingOutput,
    close_match_spending: SpendingOutput,
    transactions: Vec<Transaction>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Transaction {
    id: String,
    account_id: String,
    account_name: String,
    timestamp: String,
    description: String,
    amount: String,
    status: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    subtags: Vec<String>,
    #[serde(default)]
    annotation: Option<TransactionAnnotation>,
    #[serde(default)]
    ignored_from_spending: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TransactionAnnotation {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    subtags: Option<Vec<String>>,
    #[serde(default)]
    effective_date: Option<String>,
    /// Explicit per-transaction ignore-from-spending annotation. Lets the UI
    /// distinguish "ignored via annotation" from "ignored via rule".
    #[serde(default)]
    ignore_spending: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
struct PieSlice {
    key: String,
    total: f64,
    transaction_count: usize,
    percentage: f64,
    path: String,
    color: &'static str,
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
struct TraySnapshot {
    total_label: String,
    as_of_date: String,
    history_lines: Vec<String>,
    portfolio_breakdown_lines: Vec<String>,
    spending_lines: Vec<String>,
    transaction_lines: Vec<String>,
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
struct GitSettingsOutput {
    config_path: String,
    data_dir: String,
    git: GitRemoteSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ApplicationSettingsOutput {
    config_path: String,
    start_minimized_to_tray: bool,
    window_decorations: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ApplicationSettingsInput {
    start_minimized_to_tray: bool,
    window_decorations: String,
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
struct SyncConnectionsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    if_stale: bool,
    full_transactions: bool,
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

#[derive(Clone, Debug, Serialize, PartialEq)]
struct AiRuleTransactionInput {
    id: String,
    account_id: String,
    account_name: String,
    timestamp: String,
    description: String,
    amount: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtag: Option<String>,
    ignored_from_spending: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct AiRuleSuggestionInput {
    prompt: String,
    transactions: Vec<AiRuleTransactionInput>,
    existing_tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AiRuleSuggestionsOutput {
    model: String,
    selected_transaction_count: usize,
    suggestions: Vec<AiRuleToolCallOutput>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    response_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AiRuleToolCallOutput {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct TransactionTagTargetInput {
    account_id: String,
    transaction_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct SetTransactionTagsInput {
    transactions: Vec<TransactionTagTargetInput>,
    tags: Vec<String>,
    clear_tags: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct SetTransactionIgnoreInput {
    transactions: Vec<TransactionTagTargetInput>,
    ignore: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct SetTransactionEffectiveDateInput {
    account_id: String,
    transaction_id: String,
    effective_date: Option<String>,
    clear_effective_date: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct NetWorthDataPoint {
    date: String,
    value: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct StackedHistoryDataPoint {
    date: String,
    total: f64,
    components: Vec<StackedValue>,
}

#[derive(Clone, Debug, PartialEq)]
struct StackedValue {
    series_key: String,
    value: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveStackedSeries {
    key: String,
    label: String,
    account_id: Option<String>,
    series_type: String,
}

#[derive(Clone, Debug, PartialEq)]
struct SpendingBarChartPoint {
    label: String,
    start_date: String,
    end_date: String,
    total: f64,
    transaction_count: usize,
    segments: Vec<SpendingBarSegment>,
}

#[derive(Clone, Debug, PartialEq)]
struct SpendingBarSegment {
    key: String,
    value: f64,
    transaction_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct SpendingPeriodSelection {
    label: String,
    start_date: String,
    end_date: String,
    total: f64,
    transaction_count: usize,
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

impl RangePreset {
    /// Stable identifier used to round-trip a preset through segmented-control
    /// option values.
    fn value(self) -> &'static str {
        match self {
            Self::OneMonth => "one_month",
            Self::NinetyDays => "ninety_days",
            Self::SixMonths => "six_months",
            Self::OneYear => "one_year",
            Self::TwoYears => "two_years",
            Self::Max => "max",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SamplingGranularity {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpendingBucket {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransactionSortField {
    Date,
    Amount,
    Description,
    Tag,
    Account,
    #[allow(dead_code)]
    Counted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssetSortField {
    Name,
    Amount,
    AmountChecked,
    AmountChanged,
    PriceUpdated,
    Value,
    DayChange,
    WeekChange,
    MonthChange,
    YearChange,
}

impl AssetSortField {
    const OPTIONS: [Self; 10] = [
        Self::Name,
        Self::Amount,
        Self::Value,
        Self::PriceUpdated,
        Self::DayChange,
        Self::WeekChange,
        Self::MonthChange,
        Self::YearChange,
        Self::AmountChecked,
        Self::AmountChanged,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Name => "Asset",
            Self::Amount => "Amount",
            Self::AmountChecked => "Amount checked",
            Self::AmountChanged => "Amount changed",
            Self::PriceUpdated => "Price updated",
            Self::Value => "Value",
            Self::DayChange => "1D change",
            Self::WeekChange => "1W change",
            Self::MonthChange => "1M change",
            Self::YearChange => "1Y change",
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Amount => "amount",
            Self::AmountChecked => "amount_checked",
            Self::AmountChanged => "amount_changed",
            Self::PriceUpdated => "price_updated",
            Self::Value => "value",
            Self::DayChange => "day_change",
            Self::WeekChange => "week_change",
            Self::MonthChange => "month_change",
            Self::YearChange => "year_change",
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|field| field.value() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn label(self) -> &'static str {
        match self {
            Self::Asc => "Ascending",
            Self::Desc => "Descending",
        }
    }

    fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
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

    /// Distinct from [`Self::query_value`], which collapses `Auto` onto `Daily`
    /// for the backend query.
    fn value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|option| option.value() == value)
    }
}

impl SpendingBucket {
    const OPTIONS: [Self; 5] = [
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Quarterly,
        Self::Yearly,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Quarterly => "Quarterly",
            Self::Yearly => "Yearly",
        }
    }

    fn query_value(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|option| option.query_value() == value)
    }
}

fn main() {
    #[cfg(feature = "desktop")]
    {
        if tray::activate_existing_instance() {
            return;
        }

        configure_linux_desktop_environment();
        dioxus::LaunchBuilder::desktop()
            .with_cfg(desktop_config())
            .launch(views::App);
    }

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(views::App);
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn configure_linux_desktop_environment() {
    if std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("GDK_BACKEND", "wayland,x11");
    }

    glib::set_prgname(Some(APP_ID));
    glib::set_application_name(APP_NAME);
    if let Err(error) = gtk::init() {
        eprintln!("Failed to initialize GTK before configuring Keepbook desktop identity: {error}");
        return;
    }
    gdk::set_program_class(APP_ID);
    gtk::Window::set_default_icon_name(APP_ID);
}

#[cfg(all(feature = "desktop", not(target_os = "linux")))]
fn configure_linux_desktop_environment() {}

#[cfg(feature = "desktop")]
fn should_disable_window_decorations_for(
    config: keepbook_server::WindowDecorationsConfig,
    env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    match config {
        keepbook_server::WindowDecorationsConfig::Auto => {
            auto_should_disable_window_decorations(env_var)
        }
        keepbook_server::WindowDecorationsConfig::System => false,
        keepbook_server::WindowDecorationsConfig::Hidden => true,
    }
}

#[cfg(feature = "desktop")]
fn auto_should_disable_window_decorations(
    env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    #[cfg(target_os = "linux")]
    {
        is_hyprland_session(env_var)
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn is_hyprland_session(mut env_var: impl FnMut(&str) -> Option<std::ffi::OsString>) -> bool {
    if env_var("HYPRLAND_INSTANCE_SIGNATURE")
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ]
    .into_iter()
    .filter_map(&mut env_var)
    .any(|value| desktop_session_value_is_hyprland(&value))
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn desktop_session_value_is_hyprland(value: &std::ffi::OsStr) -> bool {
    value
        .to_string_lossy()
        .split([':', ';', ','])
        .any(|part| part.trim().eq_ignore_ascii_case("hyprland"))
}

#[cfg(feature = "desktop")]
fn desktop_config() -> dioxus::desktop::Config {
    let startup_options = desktop_startup_options();
    let config = dioxus::desktop::Config::new()
        // Dioxus's default Linux workaround forces GTK to X11. Keepbook prefers
        // native Wayland so fractional-scaled compositors do not blur the UI.
        .with_disable_dma_buf_on_wayland(false)
        .with_window(desktop_window_builder(startup_options));
    #[cfg(target_os = "linux")]
    let config = config.with_data_directory(glib::user_data_dir().join("keepbook-dioxus"));
    #[cfg(target_os = "linux")]
    let config = {
        use dioxus::desktop::tao::event_loop::EventLoopBuilder;
        use dioxus::desktop::tao::platform::unix::EventLoopBuilderExtUnix;

        let mut event_loop_builder = EventLoopBuilder::with_user_event();
        event_loop_builder.with_app_id(APP_ID);
        config.with_event_loop(event_loop_builder.build())
    };
    match dioxus::desktop::icon_from_memory::<dioxus::desktop::tao::window::Icon>(APP_ICON_PNG) {
        Ok(icon) => config.with_icon(icon),
        Err(error) => {
            eprintln!("Failed to load keepbook window icon: {error}");
            config
        }
    }
}

#[cfg(feature = "desktop")]
fn desktop_window_builder(
    startup_options: DesktopStartupOptions,
) -> dioxus::desktop::tao::window::WindowBuilder {
    let mut window = dioxus::desktop::tao::window::WindowBuilder::new()
        .with_title(APP_NAME)
        .with_visible(desktop_window_visible(startup_options));
    if should_disable_window_decorations_for(startup_options.window_decorations, |name| {
        std::env::var_os(name)
    }) {
        window = window.with_decorations(false);
    }
    window
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesktopStartupOptions {
    start_minimized_to_tray: bool,
    window_decorations: keepbook_server::WindowDecorationsConfig,
}

#[cfg(feature = "desktop")]
fn desktop_startup_options() -> DesktopStartupOptions {
    let config_path = api::native_config_path();
    let start_minimized_to_tray =
        match keepbook_server::desktop_start_minimized_to_tray(&config_path) {
            Ok(start_minimized_to_tray) => start_minimized_to_tray,
            Err(error) => {
                eprintln!("Failed to load Keepbook start-minimized config: {error:#}");
                false
            }
        };
    let window_decorations = match keepbook_server::desktop_window_decorations(&config_path) {
        Ok(window_decorations) => window_decorations,
        Err(error) => {
            eprintln!("Failed to load Keepbook window decorations config: {error:#}");
            keepbook_server::WindowDecorationsConfig::Auto
        }
    };
    DesktopStartupOptions {
        start_minimized_to_tray,
        window_decorations,
    }
}

#[cfg(feature = "desktop")]
fn desktop_window_visible(options: DesktopStartupOptions) -> bool {
    !options.start_minimized_to_tray
}

#[cfg(test)]
#[path = "../tests/unit/main_tests.rs"]
mod main_tests;
