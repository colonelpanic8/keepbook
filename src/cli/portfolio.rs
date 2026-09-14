use clap::Subcommand;

#[derive(Subcommand)]
pub enum PortfolioCommand {
    /// Calculate portfolio snapshot with valuations
    Snapshot {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Calculate as of this date (YYYY-MM-DD, default: today)
        #[arg(long)]
        date: Option<String>,

        /// Output grouping: asset, account, or both
        #[arg(long, default_value = "both")]
        group_by: String,

        /// Include per-account breakdown when grouping by asset
        #[arg(long)]
        detail: bool,

        /// Capital gains tax rate percentage for unrealized gain estimate
        #[arg(long)]
        capital_gains_tax_rate: Option<String>,

        /// Uniform percentage change to equity assets before valuation
        #[arg(long, conflicts_with = "target_pre_tax_total_value")]
        equity_change_percent: Option<String>,

        /// Uniformly scale equity assets so pre-tax total_value equals this amount
        #[arg(long, conflicts_with = "equity_change_percent")]
        target_pre_tax_total_value: Option<String>,

        /// Auto-refresh stale data (default behavior, explicit flag for scripts)
        #[arg(long, conflicts_with_all = ["offline", "dry_run", "force_refresh"])]
        auto: bool,

        /// Use cached data only, no network requests
        #[arg(long, conflicts_with_all = ["auto", "dry_run", "force_refresh"])]
        offline: bool,

        /// Show what would be refreshed without actually refreshing
        #[arg(long, conflicts_with_all = ["auto", "offline", "force_refresh"])]
        dry_run: bool,

        /// Force refresh all data regardless of staleness
        #[arg(long, conflicts_with_all = ["auto", "offline", "dry_run"])]
        force_refresh: bool,
    },

    /// Per-asset portfolio breakdown with day/week/month/year changes
    Assets {
        /// Calculate as of this date (YYYY-MM-DD, default: today)
        #[arg(long)]
        date: Option<String>,

        /// Include holding amount changes in trailing-period changes
        #[arg(long)]
        include_amount_changes: bool,
    },

    /// Map nominal net worth to after-tax net worth under latent tax scenarios
    TaxImpact {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Calculate as of this date (YYYY-MM-DD, default: today)
        #[arg(long)]
        date: Option<String>,

        /// Capital gains tax rate percentage for unrealized gain estimate
        #[arg(long)]
        capital_gains_tax_rate: Option<String>,

        /// Minimum nominal pre-tax net worth for the curve
        #[arg(long)]
        min: Option<String>,

        /// Maximum nominal pre-tax net worth for the curve
        #[arg(long)]
        max: Option<String>,

        /// Number of curve points
        #[arg(long, default_value_t = 25)]
        points: usize,
    },

    /// Track net worth over time at every change point
    History {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Start date for history (YYYY-MM-DD, YYYY-MM, YYYY, today, or relative e.g. -1y)
        #[arg(long, allow_hyphen_values = true)]
        start: Option<String>,

        /// End date for history (YYYY-MM-DD, YYYY-MM, YYYY, today, or relative e.g. -1y)
        #[arg(long, allow_hyphen_values = true)]
        end: Option<String>,

        /// Time granularity: none/full, hourly, daily, weekly, monthly, yearly (default: from config)
        #[arg(long)]
        granularity: Option<String>,

        /// Include price changes as change points (default: from config)
        #[arg(long, conflicts_with = "no_include_prices")]
        include_prices: bool,

        /// Disable price changes as change points (faster, less detailed)
        #[arg(long, conflicts_with = "include_prices")]
        no_include_prices: bool,

        /// Restrict history to a single account by id or name.
        /// Use virtual:latent_capital_gains_tax for the configured latent tax account.
        #[arg(long, conflicts_with = "connection")]
        account: Option<String>,

        /// Restrict history to accounts under a connection by id or name
        #[arg(long, conflicts_with = "account")]
        connection: Option<String>,
    },

    /// List all change points (timestamps where portfolio value could have changed)
    ChangePoints {
        /// Start date (YYYY-MM-DD, YYYY-MM, YYYY, today, or relative e.g. -1y)
        #[arg(long, allow_hyphen_values = true)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, YYYY-MM, YYYY, today, or relative e.g. -1y)
        #[arg(long, allow_hyphen_values = true)]
        end: Option<String>,

        /// Time granularity: none/full, hourly, daily, weekly, monthly, yearly (default: from config)
        #[arg(long)]
        granularity: Option<String>,

        /// Include price changes as change points (default: from config)
        #[arg(long, conflicts_with = "no_include_prices")]
        include_prices: bool,

        /// Disable price changes as change points (faster, less detailed)
        #[arg(long, conflicts_with = "include_prices")]
        no_include_prices: bool,
    },
}
