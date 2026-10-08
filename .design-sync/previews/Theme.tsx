import { Badge, ControlButton, MetricCard, OperationStatus, Theme, THEMES } from "keepbook-design";

const Sample = () => (
  <div style={{ display: "grid", gap: "var(--sp-12)" }}>
    <MetricCard label="Net worth" value="$6,814.24" detail="2026-10-08" />
    <div style={{ display: "flex", gap: "var(--sp-8)", flexWrap: "wrap" }}>
      <ControlButton primary icon="refresh">Refresh prices</ControlButton>
      <ControlButton>Select all</ControlButton>
      <Badge tone="positive">Active</Badge>
      <Badge tone="warning">Liability</Badge>
    </div>
    <OperationStatus busy message="Refreshing prices…" />
  </div>
);

export const EveryTheme = () => (
  <div style={{ display: "grid", gap: "var(--sp-12)" }}>
    {THEMES.map((theme) => (
      <Theme key={theme.id} name={theme.id}>
        <Sample />
      </Theme>
    ))}
  </div>
);
