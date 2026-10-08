import type { ChangeEventHandler } from "react";

export interface CheckboxProps {
  label: string;
  checked?: boolean;
  disabled?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement>;
}

/**
 * A compact labeled checkbox for view options and row toggles.
 *
 * Examples: "Show ignored", "Included". For a setting that takes effect
 * immediately, use `Switch` in a `SettingRow` instead.
 */
export function Checkbox({ label, checked, disabled, onChange }: CheckboxProps) {
  return (
    <label className="compact-check">
      <input type="checkbox" defaultChecked={checked} disabled={disabled} onChange={onChange} />
      {label}
    </label>
  );
}
