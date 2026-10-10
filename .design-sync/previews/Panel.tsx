import { ControlButton, Panel } from "keepbook-design";

export const WithActions = () => (
  <Panel
    title="Predictable recurring costs"
    subtitle="Active, regular outflows with stable amounts"
    actions={<ControlButton>Refresh</ControlButton>}
  >
    <p style={{ margin: 0 }}>
      Netflix, rent, and utilities recur on a fixed schedule. Verify a pattern to keep it in the
      forecast, or dismiss it to hide it from this list.
    </p>
  </Panel>
);

export const TitleOnly = () => (
  <Panel title="Accounts" subtitle="2">
    <p style={{ margin: 0, color: "var(--color-text-muted)" }}>Accounts grouped by connection appear here.</p>
  </Panel>
);

export const Saving = () => (
  <Panel title="Application" subtitle="Build" status="Saving…">
    <p style={{ margin: 0, color: "var(--color-text-muted)" }}>Settings save as soon as they change.</p>
  </Panel>
);
