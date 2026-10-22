pub mod alpha_vantage;
pub mod coincap;
pub mod coingecko;
pub mod cryptocompare;
pub mod eodhd;
pub mod frankfurter;
pub mod kalshi;
pub mod marketstack;
pub mod polymarket_us;
pub mod twelve_data;
pub mod yahoo;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use super::{AssetId, PriceKind, PricePoint};

pub use alpha_vantage::AlphaVantagePriceSource;
pub use coincap::CoinCapPriceSource;
pub use coingecko::CoinGeckoPriceSource;
pub use cryptocompare::CryptoComparePriceSource;
pub use eodhd::EodhdPriceSource;
pub use frankfurter::FrankfurterRateSource;
pub use kalshi::KalshiPriceSource;
pub use marketstack::MarketstackPriceSource;
pub use polymarket_us::PolymarketUsPriceSource;
pub use twelve_data::TwelveDataPriceSource;
pub use yahoo::YahooPriceSource;

pub(crate) fn price_point(
    asset_id: &AssetId,
    as_of_date: NaiveDate,
    timestamp: DateTime<Utc>,
    price: Decimal,
    quote_currency: &str,
    kind: PriceKind,
    source: &str,
) -> PricePoint {
    PricePoint {
        asset_id: asset_id.clone(),
        as_of_date,
        timestamp,
        price: price.normalize().to_string(),
        quote_currency: quote_currency.trim().to_uppercase(),
        kind,
        source: source.to_string(),
    }
}
