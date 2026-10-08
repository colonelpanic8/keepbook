// The theme runtime shared by the app and the React mirrors. It keeps the
// stored setting, resolves "system" mode, and sets <html data-theme> to
// "<palette>-<mode>". Generated palettes ("dynamic") store their CSS beside
// the setting so it applies before the app has loaded.
(function () {
  // Absent outside a browser, as when the design package renders on a server.
  if (typeof window === "undefined" || window.keepbookTheme) {
    return;
  }
  var SETTINGS_KEY = "keepbook-theme";
  var CSS_KEY = "keepbook-theme-css";
  var STYLE_ID = "keepbook-generated-theme";
  var systemDark = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

  function storage(method, key, value) {
    try {
      return localStorage[method](key, value);
    } catch (error) {
      return null;
    }
  }

  // Earlier versions stored a bare theme id: "fern" or "dark".
  function read() {
    var raw = storage("getItem", SETTINGS_KEY);
    if (raw === "fern") return { palette: "fern", mode: "light" };
    if (raw === "dark") return { palette: "fern", mode: "dark" };
    try {
      var settings = JSON.parse(raw);
      if (settings && typeof settings.palette === "string" && typeof settings.mode === "string") {
        return settings;
      }
    } catch (error) {}
    return { palette: "fern", mode: "system" };
  }

  function resolve(settings) {
    var dark = settings.mode === "dark" || (settings.mode === "system" && !!systemDark && systemDark.matches);
    return settings.palette + (dark ? "-dark" : "-light");
  }

  function apply() {
    var settings = read();
    var css = storage("getItem", CSS_KEY);
    var style = document.getElementById(STYLE_ID);
    if (css) {
      if (!style) {
        style = document.createElement("style");
        style.id = STYLE_ID;
        document.head.appendChild(style);
      }
      if (style.textContent !== css) style.textContent = css;
    }
    document.documentElement.dataset.theme = resolve(settings);
  }

  // `css` is the generated palette's stylesheet, or null to keep the stored one.
  function write(settings, css) {
    storage("setItem", SETTINGS_KEY, JSON.stringify(settings));
    if (css) storage("setItem", CSS_KEY, css);
    apply();
  }

  window.keepbookTheme = { read: read, write: write, apply: apply };
  apply();
  if (systemDark && systemDark.addEventListener) systemDark.addEventListener("change", apply);
  window.addEventListener("storage", apply);
})();
