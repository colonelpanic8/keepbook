import { TagPill } from "keepbook-design";

export const Applied = () => (
  <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
    <TagPill tag="Groceries" />
    <TagPill tag="Dining" />
    <TagPill tag="Travel" />
  </div>
);

export const Editing = () => (
  <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
    <TagPill tag="Groceries" removable />
    <TagPill tag="Household" removable />
    <TagPill tag="Dining" suggestion />
  </div>
);
