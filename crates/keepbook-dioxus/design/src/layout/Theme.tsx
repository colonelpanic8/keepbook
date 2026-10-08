import type { ReactNode } from "react";
import themes from "../../../assets/themes.json";

/** A palette or mode, from the shared `assets/themes.json`. */
export interface ThemeChoice {
  id: string;
  label: string;
  /** Generated from a seed color at runtime rather than defined in `styles.css`. */
  generated?: boolean;
}

/** Palettes in picker order: `fern`, `catppuccin`, `solarized`, and the generated `dynamic`. */
export const THEME_PALETTES: readonly ThemeChoice[] = themes.palettes;
/** `light`, `dark`, and `system`, which follows the OS setting. */
export const THEME_MODES: readonly ThemeChoice[] = themes.modes;

/** The stored theme setting, as `assets/theme.js` keeps it. */
export interface ThemeSettings {
  palette: string;
  mode: string;
  /** Seed color for the dynamic palette, as `#rrggbb`. */
  seed?: string;
}

/** The dynamic palette's seed until one is picked: Fern's primary. */
export const DEFAULT_SEED = "#1f6f8b";

declare global {
  interface Window {
    /** The shared theme runtime from `assets/theme.js`. */
    keepbookTheme?: {
      read(): ThemeSettings;
      write(settings: ThemeSettings, css: string | null): void;
      apply(): void;
    };
  }
}

/** The stored setting, or the default (`fern` following the system) where there is none. */
export function readThemeSettings(): ThemeSettings {
  return (typeof window !== "undefined" && window.keepbookTheme?.read()) || { palette: "fern", mode: "system" };
}

/**
 * Stores a setting and re-themes the page. Designs show the dynamic palette
 * with a sample the app generated from `DEFAULT_SEED`; the app regenerates it
 * from the user's seed or wallpaper.
 */
export function writeThemeSettings(settings: ThemeSettings): void {
  window.keepbookTheme?.write(settings, null);
}

export interface ThemeProps {
  /** One of `THEME_PALETTES`. */
  palette: string;
  mode: "light" | "dark";
  children: ReactNode;
}

/**
 * Renders a section in a specific palette and mode, on that theme's page background.
 *
 * To theme a whole design, use `ThemePicker` or set `data-theme="<palette>-<mode>"`
 * on `<html>` instead. Use `Theme` to show themes side by side.
 */
export function Theme({ palette, mode, children }: ThemeProps) {
  return (
    <div
      data-theme={`${palette}-${mode}`}
      style={{ background: "var(--color-bg)", borderRadius: "var(--radius-md)", padding: "var(--sp-16)" }}
    >
      {children}
    </div>
  );
}
