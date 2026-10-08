import type { ReactNode } from "react";
import themes from "../../../assets/themes.json";

/** The app's themes, in picker order, from the shared `assets/themes.json`. */
export const THEMES: readonly { id: string; label: string }[] = themes;

/** A theme id from `THEMES`. */
export type ThemeName = string;

const STORAGE_KEY = "keepbook-theme";

/** The selected theme: a `?theme=` URL parameter, else the app's stored choice, else `fern`. */
export function storedTheme(): ThemeName {
  const known = (value: string | null) => THEMES.find((t) => t.id === value)?.id;
  try {
    return known(new URLSearchParams(location.search).get("theme")) ?? known(localStorage.getItem(STORAGE_KEY)) ?? "fern";
  } catch {
    return "fern";
  }
}

/** Applies a theme to the whole document and remembers it, as the app's theme setting does. */
export function applyTheme(name: ThemeName): void {
  if (name === "fern") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = name;
  try {
    localStorage.setItem(STORAGE_KEY, name);
  } catch {
    // Storage can be unavailable in sandboxed frames; the theme still applies here.
  }
}

export interface ThemeProps {
  /** One of `THEMES`. */
  name: ThemeName;
  children: ReactNode;
}

/**
 * Renders a section in a specific theme, on that theme's page background.
 *
 * To theme a whole design, use `ThemePicker` or set `data-theme` on `<html>`
 * instead. Use `Theme` to show themes side by side.
 */
export function Theme({ name, children }: ThemeProps) {
  return (
    <div
      data-theme={name}
      style={{ background: "var(--color-bg)", borderRadius: "var(--radius-md)", padding: "var(--sp-16)" }}
    >
      {children}
    </div>
  );
}
