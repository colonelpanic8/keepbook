import { ThemeOptions } from "keepbook-design";

export const Fixed = () => <ThemeOptions settings={{ palette: "catppuccin", mode: "system" }} />;

export const DynamicSeed = () => <ThemeOptions settings={{ palette: "dynamic", mode: "light", seed: "#6750a4" }} />;

export const DynamicWallpaper = () => <ThemeOptions settings={{ palette: "dynamic", mode: "dark" }} wallpaper />;
