import { Badge, ControlButton, MetricCard, Theme, THEME_PALETTES } from "keepbook-design";

const Sample = () => (
  <div style={{ display: "grid", gap: "var(--sp-12)" }}>
    <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
    <div style={{ display: "flex", gap: "var(--sp-8)", flexWrap: "wrap" }}>
      <ControlButton primary icon="refresh">Refresh prices</ControlButton>
      <ControlButton>Select all</ControlButton>
      <Badge tone="positive">Active</Badge>
      <Badge tone="negative">Dismissed</Badge>
      <Badge tone="warning">Liability</Badge>
    </div>
  </div>
);

export const EveryTheme = () => (
  <div style={{ display: "grid", gap: "var(--sp-12)", gridTemplateColumns: "repeat(2, minmax(0, 1fr))" }}>
    {THEME_PALETTES.flatMap((palette) =>
      (["light", "dark"] as const).map((mode) => (
        <Theme key={`${palette.id}-${mode}`} palette={palette.id} mode={mode}>
          <strong style={{ color: "var(--color-text)" }}>
            {palette.label} · {mode}
          </strong>
          <Sample />
        </Theme>
      )),
    )}
  </div>
);
