import { Checkbox } from "keepbook-design";

export const ViewOptions = () => (
  <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
    <Checkbox label="Show ignored" checked />
    <Checkbox label="Borderline" />
    <Checkbox label="Absolute changes" />
  </div>
);
