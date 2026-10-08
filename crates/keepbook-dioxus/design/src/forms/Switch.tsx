import type { ChangeEventHandler } from "react";

export interface SwitchProps {
  /** Accessible name; the surrounding `SettingRow` shows the visible copy. */
  label: string;
  checked?: boolean;
  disabled?: boolean;
  onChange?: ChangeEventHandler<HTMLInputElement>;
}

/** An on/off switch for a setting that applies immediately. Place it in a `SettingRow`. */
export function Switch({ label, checked, disabled, onChange }: SwitchProps) {
  return (
    <label className="switch-control">
      <input type="checkbox" aria-label={label} defaultChecked={checked} disabled={disabled} onChange={onChange} />
      <span className="switch-track">
        <span className="switch-thumb" />
      </span>
    </label>
  );
}
