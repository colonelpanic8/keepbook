use clap::Args;

use super::parse_duration_arg;

#[derive(Args)]
pub struct SpendingArgs {
    /// Period granularity: daily, weekly, monthly, quarterly, yearly, range, custom
    #[arg(long, default_value = "monthly")]
    pub period: String,

    /// Bucket alignment: calendar or end-bound (default: calendar)
    #[arg(long, default_value = "calendar")]
    pub period_alignment: String,

    /// Start date (YYYY-MM-DD, default: earliest matching transaction)
    #[arg(long)]
    pub start: Option<String>,

    /// End date (YYYY-MM-DD, default: today in the selected timezone)
    #[arg(long)]
    pub end: Option<String>,

    /// Reporting currency (default: from config)
    #[arg(long)]
    pub currency: Option<String>,

    /// Timezone for bucketing and date filtering (IANA name, default: local)
    #[arg(long)]
    pub tz: Option<String>,

    /// Week start day for weekly periods: sunday or monday (default: sunday)
    #[arg(long)]
    pub week_start: Option<String>,

    /// Custom bucket size (period=custom only). Must be a positive multiple of 1d (e.g. "14d").
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    pub bucket: Option<std::time::Duration>,

    /// Filter to a single account by ID or name (mutually exclusive with --connection)
    #[arg(long)]
    pub account: Option<String>,

    /// Filter to a single connection by ID or name (mutually exclusive with --account)
    #[arg(long)]
    pub connection: Option<String>,

    /// Transaction status filter: posted, posted+pending, all (default: posted)
    #[arg(long, default_value = "posted")]
    pub status: String,

    /// Direction: outflow, inflow, net (default: outflow)
    #[arg(long, default_value = "outflow")]
    pub direction: String,

    /// Grouping: none, tag, subtag, merchant, merchant_fuzzy, account
    #[arg(long, default_value = "none")]
    pub group_by: String,

    /// Limit breakdown rows per period (when grouping)
    #[arg(long)]
    pub top: Option<usize>,

    /// Look back this many days for cached close prices / FX rates (default: 7)
    #[arg(long, default_value_t = 7)]
    pub lookback_days: u32,

    /// Include non-currency assets (equity/crypto) by valuing them using cached close prices.
    ///
    /// Default is currency-only spending (still supports FX conversion for currency txns).
    #[arg(long, default_value_t = false)]
    pub include_noncurrency: bool,

    /// Emit empty periods with total 0 and transaction_count 0.
    ///
    /// Default output is sparse (only periods with non-zero totals).
    #[arg(long, default_value_t = false)]
    pub include_empty: bool,
}

#[derive(Args)]
pub struct SpendingTagsArgs {
    /// Period granularity: daily, weekly, monthly, quarterly, yearly, range, custom
    #[arg(long, default_value = "monthly")]
    pub period: String,

    /// Bucket alignment: calendar or end-bound (default: calendar)
    #[arg(long, default_value = "calendar")]
    pub period_alignment: String,

    /// Start date (YYYY-MM-DD, default: earliest matching transaction)
    #[arg(long)]
    pub start: Option<String>,

    /// End date (YYYY-MM-DD, default: today in the selected timezone)
    #[arg(long)]
    pub end: Option<String>,

    /// Reporting currency (default: from config)
    #[arg(long)]
    pub currency: Option<String>,

    /// Timezone for bucketing and date filtering (IANA name, default: local)
    #[arg(long)]
    pub tz: Option<String>,

    /// Week start day for weekly periods: sunday or monday (default: sunday)
    #[arg(long)]
    pub week_start: Option<String>,

    /// Custom bucket size (period=custom only). Must be a positive multiple of 1d (e.g. "14d").
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    pub bucket: Option<std::time::Duration>,

    /// Filter to a single account by ID or name (mutually exclusive with --connection)
    #[arg(long)]
    pub account: Option<String>,

    /// Filter to a single connection by ID or name (mutually exclusive with --account)
    #[arg(long)]
    pub connection: Option<String>,

    /// Transaction status filter: posted, posted+pending, all (default: posted)
    #[arg(long, default_value = "posted")]
    pub status: String,

    /// Direction: outflow, inflow, net (default: outflow)
    #[arg(long, default_value = "outflow")]
    pub direction: String,

    /// Limit tag rows per period
    #[arg(long)]
    pub top: Option<usize>,

    /// Look back this many days for cached close prices / FX rates (default: 7)
    #[arg(long, default_value_t = 7)]
    pub lookback_days: u32,

    /// Include non-currency assets (equity/crypto) by valuing them using cached close prices.
    ///
    /// Default is currency-only spending (still supports FX conversion for currency txns).
    #[arg(long, default_value_t = false)]
    pub include_noncurrency: bool,

    /// Emit empty periods with total 0 and transaction_count 0.
    ///
    /// Default output is sparse (only periods with non-zero totals).
    #[arg(long, default_value_t = false)]
    pub include_empty: bool,
}
