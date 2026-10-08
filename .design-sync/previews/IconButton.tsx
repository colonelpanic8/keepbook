import { IconButton } from "keepbook-design";

export const Glyphs = () => (
  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
    <IconButton label="Previous page" glyph="‹" />
    <span style={{ fontSize: "var(--fs-small)", fontWeight: 700, color: "var(--color-text-muted)" }}>1 / 3</span>
    <IconButton label="Next page" glyph="›" />
    <IconButton label="Close" glyph="×" />
  </div>
);
