import { Switch } from "keepbook-design";

export const OnAndOff = () => (
  <div style={{ display: "flex", gap: 16 }}>
    <Switch label="Start minimized to tray" checked />
    <Switch label="Include latent capital gains tax" />
  </div>
);
