mod api;
mod dto;
mod logic;
mod views;

pub(crate) use dto::*;

#[cfg(all(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
mod tray;

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_PACKAGE_DATA_DIR: &str = "/data/user/0/org.colonelpanic.keepbook.dioxus";

const APP_CSS: &str = include_str!("../assets/styles.css");
const APP_NAME: &str = "Keepbook";
#[cfg(feature = "desktop")]
const APP_ID: &str = "org.colonelpanic.keepbook.dioxus";
#[cfg(feature = "desktop")]
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/keepbook-icon-64.png");
const SSH_KEY_FILE_PICKER_BRIDGE_JS: &str = r#"
(function () {
  if (window.__keepbookSshKeyPickerBridgeInstalled) {
    return;
  }
  window.__keepbookSshKeyPickerBridgeInstalled = true;
  var maxKeyBytes = 65536;

  function emitPayload(payload) {
    var sink = document.getElementById("ssh-private-key-file-payload");
    if (!sink) {
      return;
    }
    sink.value = "";
    sink.value = JSON.stringify(payload);
    sink.dispatchEvent(new Event("input", { bubbles: true }));
  }

  document.addEventListener("click", function (event) {
    var target = event.target;
    if (
      target instanceof HTMLInputElement &&
      target.id === "ssh-private-key-file-input" &&
      target.type === "file"
    ) {
      event.stopImmediatePropagation();
    }
  }, true);

  document.addEventListener("change", function (event) {
    var target = event.target;
    if (
      !(target instanceof HTMLInputElement) ||
      target.id !== "ssh-private-key-file-input" ||
      target.type !== "file"
    ) {
      return;
    }

    var file = target.files && target.files[0];
    if (!file) {
      emitPayload({ error: "No SSH key file selected." });
      return;
    }
    if (file.size > maxKeyBytes) {
      emitPayload({ error: "SSH key file is too large. Pick a private key file under 64 KB." });
      return;
    }

    emitPayload({ status: "Reading SSH key file " + file.name + "..." });

    var reader = new FileReader();
    reader.onload = function () {
      emitPayload({
        name: file.name,
        contents: String(reader.result || "")
      });
    };
    reader.onerror = function () {
      emitPayload({ error: "Key file read failed." });
    };
    reader.readAsText(file);
  }, true);
})();
"#;
const CONTEXT_MENU_COPY_BRIDGE_JS: &str = r#"
(function () {
  if (window.__keepbookContextMenuCopyBridgeInstalled) {
    return;
  }
  window.__keepbookContextMenuCopyBridgeInstalled = true;

  // Dioxus desktop suppresses the native context menu in release builds by
  // registering a bubble-phase `contextmenu` handler on `document` that calls
  // preventDefault(). That also removes the "Copy" entry, so selected text
  // (such as sync error messages) can be highlighted but not copied. Re-enable
  // the native menu whenever there is an active text selection or an editable
  // target by stopping the event during the capture phase, before Dioxus's
  // bubble-phase handler runs.
  document.addEventListener(
    "contextmenu",
    function (event) {
      var selection = window.getSelection();
      var hasSelection =
        selection && !selection.isCollapsed && selection.toString().length > 0;
      var target = event.target;
      var isEditable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target && target.isContentEditable);
      if (hasSelection || isEditable) {
        event.stopImmediatePropagation();
      }
    },
    true
  );
})();
"#;
const THEME_BOOTSTRAP_JS: &str = r#"
(function () {
  if (window.__keepbookThemeBootstrapInstalled) {
    return;
  }
  window.__keepbookThemeBootstrapInstalled = true;

  // Apply the persisted theme as early as possible so the app does not flash the
  // default "fern" palette before the Rust theme state loads. "fern" is the
  // `:root` default, so only non-fern themes need an explicit attribute.
  try {
    var stored = localStorage.getItem("keepbook-theme");
    if (stored && stored !== "fern") {
      document.documentElement.dataset.theme = stored;
    }
  } catch (error) {}
})();
"#;
#[cfg(target_arch = "wasm32")]
const API_BASE: &str = match option_env!("KEEPBOOK_API_BASE") {
    Some(value) => value,
    None => "http://127.0.0.1:8799",
};
const DEFAULT_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SAMPLING_GRANULARITY: SamplingGranularity = SamplingGranularity::Weekly;
const DEFAULT_SPENDING_RANGE_PRESET: RangePreset = RangePreset::OneYear;
const DEFAULT_SPENDING_BUCKET: SpendingBucket = SpendingBucket::Monthly;

fn main() {
    #[cfg(feature = "desktop")]
    {
        if tray::activate_existing_instance() {
            return;
        }

        configure_linux_desktop_environment();
        dioxus::LaunchBuilder::desktop()
            .with_cfg(desktop_config())
            .launch(views::App);
    }

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(views::App);
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn configure_linux_desktop_environment() {
    if std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland" {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("GDK_BACKEND", "wayland,x11");
    }

    glib::set_prgname(Some(APP_ID));
    glib::set_application_name(APP_NAME);
    if let Err(error) = gtk::init() {
        eprintln!("Failed to initialize GTK before configuring Keepbook desktop identity: {error}");
        return;
    }
    gdk::set_program_class(APP_ID);
    gtk::Window::set_default_icon_name(APP_ID);
}

#[cfg(all(feature = "desktop", not(target_os = "linux")))]
fn configure_linux_desktop_environment() {}

#[cfg(feature = "desktop")]
fn should_disable_window_decorations_for(
    config: keepbook_server::WindowDecorationsConfig,
    env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    match config {
        keepbook_server::WindowDecorationsConfig::Auto => {
            auto_should_disable_window_decorations(env_var)
        }
        keepbook_server::WindowDecorationsConfig::System => false,
        keepbook_server::WindowDecorationsConfig::Hidden => true,
    }
}

#[cfg(feature = "desktop")]
fn auto_should_disable_window_decorations(
    env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    #[cfg(target_os = "linux")]
    {
        is_hyprland_session(env_var)
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn is_hyprland_session(mut env_var: impl FnMut(&str) -> Option<std::ffi::OsString>) -> bool {
    if env_var("HYPRLAND_INSTANCE_SIGNATURE")
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ]
    .into_iter()
    .filter_map(&mut env_var)
    .any(|value| desktop_session_value_is_hyprland(&value))
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
fn desktop_session_value_is_hyprland(value: &std::ffi::OsStr) -> bool {
    value
        .to_string_lossy()
        .split([':', ';', ','])
        .any(|part| part.trim().eq_ignore_ascii_case("hyprland"))
}

#[cfg(feature = "desktop")]
fn desktop_config() -> dioxus::desktop::Config {
    let startup_options = desktop_startup_options();
    let config = dioxus::desktop::Config::new()
        // Dioxus's default Linux workaround forces GTK to X11. Keepbook prefers
        // native Wayland so fractional-scaled compositors do not blur the UI.
        .with_disable_dma_buf_on_wayland(false)
        .with_window(desktop_window_builder(startup_options));
    #[cfg(target_os = "linux")]
    let config = config.with_data_directory(glib::user_data_dir().join("keepbook-dioxus"));
    #[cfg(target_os = "linux")]
    let config = {
        use dioxus::desktop::tao::event_loop::EventLoopBuilder;
        use dioxus::desktop::tao::platform::unix::EventLoopBuilderExtUnix;

        let mut event_loop_builder = EventLoopBuilder::with_user_event();
        event_loop_builder.with_app_id(APP_ID);
        config.with_event_loop(event_loop_builder.build())
    };
    match dioxus::desktop::icon_from_memory::<dioxus::desktop::tao::window::Icon>(APP_ICON_PNG) {
        Ok(icon) => config.with_icon(icon),
        Err(error) => {
            eprintln!("Failed to load keepbook window icon: {error}");
            config
        }
    }
}

#[cfg(feature = "desktop")]
fn desktop_window_builder(
    startup_options: DesktopStartupOptions,
) -> dioxus::desktop::tao::window::WindowBuilder {
    let mut window = dioxus::desktop::tao::window::WindowBuilder::new()
        .with_title(APP_NAME)
        .with_visible(desktop_window_visible(startup_options));
    if should_disable_window_decorations_for(startup_options.window_decorations, |name| {
        std::env::var_os(name)
    }) {
        window = window.with_decorations(false);
    }
    window
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesktopStartupOptions {
    start_minimized_to_tray: bool,
    window_decorations: keepbook_server::WindowDecorationsConfig,
}

#[cfg(feature = "desktop")]
fn desktop_startup_options() -> DesktopStartupOptions {
    let config_path = api::native_config_path();
    let start_minimized_to_tray =
        match keepbook_server::desktop_start_minimized_to_tray(&config_path) {
            Ok(start_minimized_to_tray) => start_minimized_to_tray,
            Err(error) => {
                eprintln!("Failed to load Keepbook start-minimized config: {error:#}");
                false
            }
        };
    let window_decorations = match keepbook_server::desktop_window_decorations(&config_path) {
        Ok(window_decorations) => window_decorations,
        Err(error) => {
            eprintln!("Failed to load Keepbook window decorations config: {error:#}");
            keepbook_server::WindowDecorationsConfig::Auto
        }
    };
    DesktopStartupOptions {
        start_minimized_to_tray,
        window_decorations,
    }
}

#[cfg(feature = "desktop")]
fn desktop_window_visible(options: DesktopStartupOptions) -> bool {
    !options.start_minimized_to_tray
}

#[cfg(test)]
#[path = "../tests/unit/main_tests.rs"]
mod main_tests;
