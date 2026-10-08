import { useState } from "react";
import { SegmentedControl } from "../actions/SegmentedControl";
import { SettingRow } from "../forms/SettingRow";
import { applyTheme, storedTheme, THEMES, type ThemeName } from "./Theme";

export interface ThemePickerProps {
  /** Selected theme; defaults to the stored choice. */
  selected?: ThemeName;
  onSelect?: (name: ThemeName) => void;
}

/**
 * The Settings theme row: picking a theme re-themes the whole page and remembers the choice.
 *
 * Mirrors the Theme setting in `views/graph_settings.rs`, including its
 * `keepbook-theme` storage key, so every preview card that follows the stored
 * theme switches with it.
 */
export function ThemePicker({ selected, onSelect }: ThemePickerProps) {
  const [current, setCurrent] = useState<ThemeName>(selected ?? storedTheme());
  return (
    <SettingRow stacked title="Theme" description="Appearance of the app">
      <SegmentedControl
        className="setting-segmented"
        label="Theme"
        options={THEMES.map((t) => ({ value: t.id, label: t.label }))}
        selected={current}
        onSelect={(value) => {
          const name = value as ThemeName;
          setCurrent(name);
          applyTheme(name);
          onSelect?.(name);
        }}
      />
    </SettingRow>
  );
}
