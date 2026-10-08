/** Joins class names, dropping falsy entries. */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/**
 * The categorical `--series-N` palette shared by chart series and spending
 * tags, assigned by position. Mirrors `series_color` in `src/logic/palette.rs`.
 */
export function seriesColor(index: number): string {
  return `var(--series-${(index % 10) + 1})`;
}
