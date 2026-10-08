import { ControlButton } from "keepbook-design";

export const Variants = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <ControlButton primary icon="refresh">Refresh prices</ControlButton>
    <ControlButton icon="git-branch">Git sync</ControlButton>
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
