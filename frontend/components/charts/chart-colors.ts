export const CHART_ACCENT_RGB = { r: 46, g: 204, b: 113 }; // Emerald green (net worth growth)
export const CHART_NEGATIVE_RGB = { r: 231, g: 76, b: 60 }; // Red (negative changes)
export const CHART_BAR_RGB = { r: 52, g: 152, b: 219 }; // Blue (spending bars)

function rgba(c: { r: number; g: number; b: number }, a: number): string {
  return `rgba(${c.r}, ${c.g}, ${c.b}, ${a})`;
}

export const chartColors = {
  accent: rgba(CHART_ACCENT_RGB, 1),
  accentDim: rgba(CHART_ACCENT_RGB, 0.3),
  accentFill: rgba(CHART_ACCENT_RGB, 0.15),
  negative: rgba(CHART_NEGATIVE_RGB, 1),
  negativeDim: rgba(CHART_NEGATIVE_RGB, 0.3),
  bar: rgba(CHART_BAR_RGB, 1),
  barDim: rgba(CHART_BAR_RGB, 0.6),
  gridLine: "rgba(255, 255, 255, 0.16)",
  label: "rgba(255, 255, 255, 0.6)",
  labelStrong: "rgba(255, 255, 255, 0.78)",
};

export function chartAxisColors(isDark: boolean) {
  return isDark
    ? {
        gridLine: "rgba(255, 255, 255, 0.18)",
        label: "rgba(255, 255, 255, 0.62)",
        labelStrong: "rgba(255, 255, 255, 0.82)",
        tick: "rgba(255, 255, 255, 0.86)",
      }
    : {
        gridLine: "rgba(17, 24, 39, 0.16)",
        label: "rgba(17, 24, 39, 0.62)",
        labelStrong: "rgba(17, 24, 39, 0.78)",
        tick: "rgba(17, 24, 39, 0.86)",
      };
}

export const chartDefaults = {
  height: 300,
  domainPadding: { top: 30, bottom: 16, left: 36, right: 36 },
};
