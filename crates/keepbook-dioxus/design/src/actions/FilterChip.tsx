export interface FilterChipProps {
  /** What is being filtered, e.g. "Focused: Groceries · 2026-06". */
  label: string;
  /** Tooltip describing what clearing does. */
  title: string;
  onClear?: () => void;
}

/**
 * A pill shown only while a filter is active; clicking it clears the filter.
 *
 * Put it in its own row under the controls it summarizes, never inside an
 * option group. Mirrors `components/actions/filter_chip.rs`.
 */
export function FilterChip({ label, title, onClear }: FilterChipProps) {
  return (
    <div className="filter-chip-row">
      <button className="filter-clear-chip" type="button" title={title} onClick={onClear}>
        <span className="filter-clear-chip-text">{label}</span>
        <span aria-hidden="true">✕</span>
      </button>
    </div>
  );
}
