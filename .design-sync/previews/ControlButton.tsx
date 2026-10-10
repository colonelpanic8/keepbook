import { ControlButton } from "keepbook-design";

export const Variants = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <ControlButton primary icon="refresh">Refresh prices</ControlButton>
    <ControlButton icon="refresh">Refresh balances</ControlButton>
    <ControlButton>Select all</ControlButton>
    <ControlButton danger>Dismiss</ControlButton>
  </div>
);

export const States = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <ControlButton primary busy>Refreshing</ControlButton>
    <ControlButton disabled>Clear</ControlButton>
    <ControlButton selected>Monthly</ControlButton>
  </div>
);

export const Small = () => (
  <div style={{ display: "flex", gap: 8 }}>
    <ControlButton small>Add tag</ControlButton>
    <ControlButton small>Apply to 3</ControlButton>
  </div>
);

export const Feedback = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <ControlButton icon="refresh" feedback={{ tone: "busy", label: "Refreshing…" }}>Refresh prices</ControlButton>
    <ControlButton icon="refresh" feedback={{ tone: "done", label: "Refreshed", detail: "Refreshed 14 prices." }}>
      Refresh prices
    </ControlButton>
    <ControlButton icon="refresh" feedback={{ tone: "failed", label: "Failed", detail: "Price refresh failed: quote source unavailable." }}>
      Refresh prices
    </ControlButton>
    <ControlButton feedback={{ tone: "done", label: "", detail: "Refreshed 3 balances." }}>Balances</ControlButton>
  </div>
);
