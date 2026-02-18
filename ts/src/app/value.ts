import { Decimal } from '../decimal.js';
import { decStrRounded } from './format.js';
import { Asset, type AssetType } from '../models/asset.js';
import type { MarketDataService } from '../market-data/service.js';

export type MissingMarketData = 'price' | 'fx';

export type ValueInReportingCurrencyResult = {
  value: string | null;
  missing: MissingMarketData | null;
};

export async function valueInReportingCurrencyDetailed(
  marketData: MarketDataService,
  asset: AssetType,
  amount: string,
  reportingCurrency: string,
  asOfDate: string,
  currencyDecimals: number | undefined,
): Promise<ValueInReportingCurrencyResult> {
  const amountValue = new Decimal(amount);
  const reporting = reportingCurrency.trim().toUpperCase();
  const normalized = Asset.normalized(asset);

  if (normalized.type === 'currency') {
    if (normalized.iso_code.toUpperCase() === reporting) {
      return { value: decStrRounded(amountValue, currencyDecimals), missing: null };
    }

    const rate = await marketData.fxFromStore(normalized.iso_code, reporting, asOfDate);
    if (rate === null) return { value: null, missing: 'fx' };
    return {
      value: decStrRounded(amountValue.times(new Decimal(rate.rate)), currencyDecimals),
      missing: null,
    };
  }

  const price = await marketData.priceFromStore(normalized, asOfDate);
  if (price === null) return { value: null, missing: 'price' };

  const valueInQuote = amountValue.times(new Decimal(price.price));
  if (price.quote_currency.toUpperCase() === reporting) {
    return { value: decStrRounded(valueInQuote, currencyDecimals), missing: null };
  }

  const rate = await marketData.fxFromStore(price.quote_currency, reporting, asOfDate);
  if (rate === null) return { value: null, missing: 'fx' };
  return {
    value: decStrRounded(valueInQuote.times(new Decimal(rate.rate)), currencyDecimals),
    missing: null,
  };
}

export async function valueInReportingCurrency(
  marketData: MarketDataService,
  asset: AssetType,
  amount: string,
  reportingCurrency: string,
  asOfDate: string,
  currencyDecimals: number | undefined,
): Promise<string | null> {
  const res = await valueInReportingCurrencyDetailed(
    marketData,
    asset,
    amount,
    reportingCurrency,
    asOfDate,
    currencyDecimals,
  );
  return res.value;
}
