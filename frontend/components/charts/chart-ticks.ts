export function tickCountForSpan(
  span: number | undefined,
  minSpacing: number,
  minCount: number,
  maxCount: number,
): number {
  if (!Number.isFinite(span) || span === undefined || span <= 0) {
    return minCount;
  }

  const count = Math.floor(span / minSpacing) + 1;
  return Math.max(minCount, Math.min(maxCount, count));
}

export function evenlySpacedIndices(length: number, maxCount: number): number[] {
  if (length <= 0 || maxCount <= 0) return [];
  if (length === 1) return [0];

  const count = Math.max(2, Math.min(length, Math.floor(maxCount)));
  if (count >= length) {
    return Array.from({ length }, (_, index) => index);
  }

  const last = length - 1;
  const ticks: number[] = [];
  for (let i = 0; i < count; i += 1) {
    const index = Math.round((last * i) / (count - 1));
    if (ticks[ticks.length - 1] !== index) {
      ticks.push(index);
    }
  }
  return ticks;
}
