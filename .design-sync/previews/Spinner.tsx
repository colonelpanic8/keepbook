import { Spinner } from "keepbook-design";

export const Sizes = () => (
  <div style={{ display: "flex", gap: 16, alignItems: "center", color: "var(--color-text-muted)" }}>
    <Spinner size="small" />
    <Spinner />
    <Spinner size="large" />
    <span style={{ fontSize: "var(--fs-small)" }}>Switching repository…</span>
  </div>
);
