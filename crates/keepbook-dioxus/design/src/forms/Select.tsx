import type { ChangeEventHandler } from "react";
import { cx } from "../cx";

export interface SelectProps {
  /** Field label above the select; omit inside a `SettingRow`. */
  label?: string;
  options: string[];
  value?: string;
  small?: boolean;
  disabled?: boolean;
  onChange?: ChangeEventHandler<HTMLSelectElement>;
}

/**
 * A themed dropdown with its own caret, optionally labeled above.
 *
 * Use it for long or open-ended choices; use `SegmentedControl` for two to six
 * fixed options.
 */
export function Select({ label, options, value, small, disabled, onChange }: SelectProps) {
  const control = (
    <select className={cx("control-input", small && "small")} defaultValue={value} disabled={disabled} onChange={onChange}>
      {options.map((option) => (
        <option key={option}>{option}</option>
      ))}
    </select>
  );
  if (!label) return control;
  return (
    <label className="control-field">
      <span>{label}</span>
      {control}
    </label>
  );
}
