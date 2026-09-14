use clap::Subcommand;

#[derive(Subcommand)]
pub enum MarketDataCommand {
    /// Fetch historical prices for assets in scope
    Fetch {
        /// Account ID or name (mutually exclusive with --connection)
        #[arg(long)]
        account: Option<String>,

        /// Connection ID or name (mutually exclusive with --account)
        #[arg(long)]
        connection: Option<String>,

        /// Start date (YYYY-MM-DD, default: earliest balance date in scope)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Interval for backfill: daily, weekly, monthly, yearly/annual (default: monthly)
        #[arg(long, default_value = "monthly")]
        interval: String,

        /// Look back this many days when a close price is missing (default: 7)
        #[arg(long, default_value_t = 7)]
        lookback_days: u32,

        /// Delay (ms) between price fetches to avoid rate limits (default: 0)
        #[arg(long, default_value_t = 0)]
        request_delay_ms: u64,

        /// Base currency for FX rates (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Disable FX rate fetching
        #[arg(long)]
        no_fx: bool,
    },
}
