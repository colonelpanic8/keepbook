import { useState } from "react";
import { readThemeSettings, writeThemeSettings, type ThemeSettings } from "./Theme";
import { ThemeOptions } from "./ThemeOptions";

export interface ThemePickerProps {
  onChange?: (settings: ThemeSettings) => void;
}

/**
 * The Settings theme rows bound to the stored setting: a change re-themes the whole page and is remembered.
 *
 * Mirrors `ThemePicker` in `components/layout/theme_picker.rs` and shares its
 * runtime, `assets/theme.js`, so every preview card that follows the stored
 * theme switches with it.
 */
export function ThemePicker({ onChange }: ThemePickerProps) {
  const [settings, setSettings] = useState<ThemeSettings>(readThemeSettings);
  return (
    <ThemeOptions
      settings={settings}
      onChange={(next) => {
        setSettings(next);
        writeThemeSettings(next);
        onChange?.(next);
      }}
    />
  );
}
