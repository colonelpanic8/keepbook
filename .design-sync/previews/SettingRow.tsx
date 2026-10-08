import { SegmentedControl, Select, SettingRow, Switch } from "keepbook-design";

export const WithSwitch = () => (
  <SettingRow title="Start minimized to tray" description="Launch Keepbook in the background and open it from the tray icon">
    <Switch label="Start minimized to tray" checked />
  </SettingRow>
);

export const WithSelect = () => (
  <SettingRow stacked title="Window decorations" description="Auto hides the system title bar on Hyprland">
    <Select options={["Auto", "System", "Hidden"]} value="Auto" />
  </SettingRow>
);

export const WithOptions = () => (
  <SettingRow stacked title="Mode" description="Light, dark, or matching the system">
    <SegmentedControl
      className="setting-segmented"
      label="Mode"
      selected="system"
      options={[
        { value: "light", label: "Light" },
        { value: "dark", label: "Dark" },
        { value: "system", label: "System" },
      ]}
    />
  </SettingRow>
);
