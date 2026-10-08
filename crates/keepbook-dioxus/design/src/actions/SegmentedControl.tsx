import { useState, type CSSProperties } from "react";
import { cx } from "../cx";

export interface SegmentedOption {
  value: string;
  label: string;
}

export interface SegmentedControlProps {
  /** Overline label; also the group's accessible name. */
  label: string;
  options: SegmentedOption[];
  /** Selected option value; defaults to the first option. */
  selected?: string;
  onSelect?: (value: string) => void;
  className?: string;
}

/**
 * A labeled one-row group of mutually exclusive options.
 *
 * Use it for every one-of-N choice (range presets, bucket size, view tabs);
 * never use a wrapping row of buttons. The row never wraps: labels shrink and
 * then ellipsize instead. Mirrors `components/actions/segmented_control.rs`.
 */
export function SegmentedControl({ label, options, selected, onSelect, className }: SegmentedControlProps) {
  const [current, setCurrent] = useState(selected ?? options[0]?.value);
  const value = selected ?? current;
  return (
    <div className={cx("segmented-field", className)}>
      <span className="control-label segmented-label">{label}</span>
      <div
        className="segmented-control"
        role="group"
        aria-label={label}
        style={{ "--segment-count": Math.max(options.length, 1) } as CSSProperties}
      >
        {options.map((option) => (
          <button
            key={option.value}
            className={cx("segment", option.value === value && "selected")}
            type="button"
            title={option.label}
            aria-pressed={option.value === value}
            onClick={() => {
              setCurrent(option.value);
              onSelect?.(option.value);
            }}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
