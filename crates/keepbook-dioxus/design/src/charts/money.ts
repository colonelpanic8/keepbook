/** Mirrors `format_money_display` in `src/logic/format.rs`: USD shows `$`, other currencies a code prefix. */
export function money(value: number, currency: string, decimals: number, suffix = ""): string {
  const amount = Math.abs(value).toLocaleString("en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
  const sign = value < 0 ? "-" : "";
  const code = currency.trim().toUpperCase();
  return code === "USD" ? `${sign}$${amount}${suffix}` : `${code} ${sign}${amount}${suffix}`;
}

/** Mirrors `format_compact_money`: one decimal with a K, M, or B suffix. */
export function compactMoney(value: number, currency: string): string {
  const abs = Math.abs(value);
  if (abs >= 1e9) return money(value / 1e9, currency, 1, "B");
  if (abs >= 1e6) return money(value / 1e6, currency, 1, "M");
  if (abs >= 1e3) return money(value / 1e3, currency, 1, "K");
  return money(value, currency, 1);
}
