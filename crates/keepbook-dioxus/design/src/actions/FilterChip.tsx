export interface FilterChipProps {
  /** What is being filtered, e.g. "Groceries · 2026-06". */
  label: string;
  onClear?: () => void;
}

/**
 * A pill shown only while a filter is active; clicking it clears the filter.
 *
 * Put it in its own row under the controls it summarizes, never inside an
 * option group.
 */
export function FilterChip({ label, onClear }: FilterChipProps) {
  return (
    <div className="filter-chip-row">
      <button className="filter-clear-chip" type="button" title="Clear filter" onClick={onClear}>
        <span className="filter-clear-chip-text">{label}</span>
        <span aria-hidden="true">×</span>
      </button>
    </div>
  );
}
