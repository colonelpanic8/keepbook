import { Badge } from "keepbook-design";

export const Tones = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <Badge>Not counted</Badge>
    <Badge tone="positive">Active</Badge>
    <Badge tone="negative">Dismissed</Badge>
    <Badge tone="warning">Liability</Badge>
  </div>
);

export const InAHeader = () => (
  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
    <strong>Demo Bank</strong>
    <Badge tone="positive">2/2 active</Badge>
  </div>
);
