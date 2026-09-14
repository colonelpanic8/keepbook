use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct RepositoryRegistry {
    pub(crate) config_path: String,
    #[serde(default)]
    pub(crate) device_config_path: String,
    pub(crate) active_repository: Option<String>,
    pub(crate) repositories: Vec<Repository>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Repository {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) remote: String,
    pub(crate) branch: String,
    pub(crate) active: bool,
    pub(crate) cloned: bool,
    pub(crate) commit: Option<String>,
    #[serde(default)]
    pub(crate) managed: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct AddRepositoryInput {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) remote: String,
    pub(crate) branch: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct HistoryDefaults {
    pub(crate) portfolio_granularity: String,
    pub(crate) change_points_granularity: String,
    pub(crate) include_prices: bool,
    pub(crate) graph_range: String,
    pub(crate) graph_granularity: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
pub(crate) struct FilteringSettings {
    pub(crate) latent_capital_gains_tax: LatentCapitalGainsTaxFilter,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct LatentCapitalGainsTaxFilter {
    pub(crate) configured_enabled: bool,
    pub(crate) effective_enabled: bool,
    pub(crate) override_enabled: Option<bool>,
    pub(crate) rate_configured: bool,
    pub(crate) account_name: String,
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
pub(crate) struct FilterOverrides {
    pub(crate) include_latent_capital_gains_tax: Option<bool>,
    pub(crate) account_portfolio_exclusions: Vec<AccountPortfolioExclusionOverride>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct AccountPortfolioExclusionOverride {
    pub(crate) account_id: String,
    pub(crate) exclude_from_portfolio: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct GitRemoteSettings {
    pub(crate) host: String,
    pub(crate) repo: String,
    pub(crate) branch: String,
    pub(crate) ssh_user: String,
    #[serde(default)]
    pub(crate) ssh_key_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct GitSettingsOutput {
    pub(crate) config_path: String,
    pub(crate) data_dir: String,
    pub(crate) git: GitRemoteSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct ApplicationSettingsOutput {
    pub(crate) config_path: String,
    pub(crate) start_minimized_to_tray: bool,
    pub(crate) window_decorations: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ApplicationSettingsInput {
    pub(crate) start_minimized_to_tray: bool,
    pub(crate) window_decorations: String,
}
