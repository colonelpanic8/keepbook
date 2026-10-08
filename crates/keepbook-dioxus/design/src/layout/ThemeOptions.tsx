import { SegmentedControl } from "../actions/SegmentedControl";
import { SettingRow } from "../forms/SettingRow";
import { TextInput } from "../forms/TextInput";
import { DEFAULT_SEED, THEME_MODES, THEME_PALETTES, type ThemeChoice, type ThemeSettings } from "./Theme";

export interface ThemeOptionsProps {
  settings: ThemeSettings;
  /** The system supplies the dynamic seed (Android 12 and later), so there is no seed picker. */
  wallpaper?: boolean;
  onChange?: (settings: ThemeSettings) => void;
}

const options = (choices: readonly ThemeChoice[]) => choices.map((c) => ({ value: c.id, label: c.label }));

/**
 * The Settings theme rows: palette, mode, and the dynamic palette's seed color.
 *
 * Mirrors `ThemeOptions` in `components/layout/theme_picker.rs`.
 */
export function ThemeOptions({ settings, wallpaper = false, onChange }: ThemeOptionsProps) {
  const dynamic = settings.palette === "dynamic";
  const description = dynamic
    ? wallpaper
      ? "Material You colors from your wallpaper"
      : "Material You colors from a seed color"
    : "Colors of the app";
  return (
    <>
      <SettingRow stacked title="Theme" description={description}>
        <SegmentedControl
          className="setting-segmented"
          label="Theme"
          options={options(THEME_PALETTES)}
          selected={settings.palette}
          onSelect={(palette) => onChange?.({ ...settings, palette })}
        />
      </SettingRow>
      <SettingRow stacked title="Mode" description="Light, dark, or matching the system">
        <SegmentedControl
          className="setting-segmented"
          label="Mode"
          options={options(THEME_MODES)}
          selected={settings.mode}
          onSelect={(mode) => onChange?.({ ...settings, mode })}
        />
      </SettingRow>
      {dynamic && !wallpaper && (
        <SettingRow title="Seed color" description="The dynamic palette is generated from it">
          <TextInput
            label="Seed"
            kind="color"
            value={settings.seed ?? DEFAULT_SEED}
            onChange={(event) => onChange?.({ ...settings, seed: event.target.value })}
          />
        </SettingRow>
      )}
    </>
  );
}
