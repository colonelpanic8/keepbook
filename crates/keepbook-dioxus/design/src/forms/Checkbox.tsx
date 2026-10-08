import type { ChangeEventHandler } from "react";
import { cx } from "../cx";

export interface CheckboxProps {
  label: string;
  checked?: boolean;
  disabled?: boolean;
  className?: string;
  onChange?: ChangeEventHandler<HTMLInputElement>;
}

/**
 * A compact labeled checkbox for view options and row toggles.
 *
 * Examples: "Show ignored", "Included". For a setting that takes effect
 * immediately, use `Switch` in a `SettingRow` instead. Mirrors
 * `components/forms/checkbox.rs`.
 */
export function Checkbox({ label, checked, disabled, className, onChange }: CheckboxProps) {
  return (
    <label className={cx("compact-check", className)}>
      <input type="checkbox" defaultChecked={checked} disabled={disabled} onChange={onChange} />
      <span>{label}</span>
    </label>
  );
}
