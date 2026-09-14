mod ai_rules;
mod dto;
mod git;
#[cfg(feature = "http")]
mod routes;
mod settings;
mod state;

pub use ai_rules::{
    AiRuleSuggestionInput, AiRuleSuggestionsOutput, AiRuleToolCallOutput, AiRuleTransactionInput,
};
pub use dto::{
    AddRepositoryInput, ApplicationSettingsInput, ApplicationSettingsOutput, AssetsQuery,
    ConfigOutput, FilteringOutput, GitRemoteSettings, GitRepoState, GitSettingsInput,
    GitSettingsOutput, GitSyncCancelToken, GitSyncInput, GitSyncOutput, HealthOutput,
    HistoryDefaultsOutput, HistoryQuery, LatentCapitalGainsTaxFilterOutput, OverviewOutput,
    OverviewQuery, ProposedTransactionEditsQuery, RecurringTransactionReviewInput,
    RecurringTransactionsQuery, RepositoryOutput, RepositoryRegistryOutput, SpendingQuery,
    SyncConnectionsInput, SyncPricesInput, TransactionEffectiveDateInput,
    TransactionIgnoreBatchInput, TransactionQuery, TransactionSubtagsBatchInput,
    TransactionTagTargetInput, TransactionTagsBatchInput, TraySnapshotOutput,
};
pub use keepbook::app::ReviewedRecurringTransactionOutput;
pub use keepbook::config::WindowDecorationsConfig;
#[cfg(feature = "http")]
pub use routes::{router, serve, ApiError};
pub use settings::{
    default_listen_addr, default_server_config_path, desktop_start_minimized_to_tray,
    desktop_window_decorations,
};
pub use state::{active_repository_config_path, default_app_config_path, ApiState};
