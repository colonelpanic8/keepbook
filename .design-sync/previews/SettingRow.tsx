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
  <SettingRow stacked title="Theme" description="Appearance of the app">
    <SegmentedControl
      className="setting-segmented"
      label="Theme"
      selected="fern"
      options={[
        { value: "fern", label: "Fern" },
        { value: "dark", label: "Dark" },
      ]}
    />
  </SettingRow>
);
