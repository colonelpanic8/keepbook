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
  gridLine: "rgba(255, 255, 255, 0.1)",
  label: "rgba(255, 255, 255, 0.6)",
};

export const chartDefaults = {
  height: 300,
  domainPadding: { top: 30, bottom: 10, left: 10, right: 10 },
};
