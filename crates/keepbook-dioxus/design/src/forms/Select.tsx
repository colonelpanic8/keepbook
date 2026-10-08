import type { ChangeEventHandler } from "react";
import { cx } from "../cx";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps {
  /** Field label above the select; omit inside a `SettingRow`. */
  label?: string;
  /** Options; a plain string is both the value and the label. */
  options: (string | SelectOption)[];
  value?: string;
  small?: boolean;
  disabled?: boolean;
  className?: string;
  onChange?: ChangeEventHandler<HTMLSelectElement>;
}

export const toSelectOption = (option: string | SelectOption): SelectOption =>
  typeof option === "string" ? { value: option, label: option } : option;

/**
 * A themed dropdown with its own caret, optionally labeled above.
 *
 * Use it for long or open-ended choices; use `SegmentedControl` for two to six
 * fixed options. Mirrors `components/forms/select.rs`.
 */
export function Select({ label, options, value, small, disabled, className, onChange }: SelectProps) {
  const control = (
    <select
      className={cx("control-input", small && "small", className)}
      aria-label={label}
      defaultValue={value}
      disabled={disabled}
      onChange={onChange}
    >
      {options.map(toSelectOption).map((option) => (
        <option key={option.value} value={option.value} disabled={option.disabled}>
          {option.label}
        </option>
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
