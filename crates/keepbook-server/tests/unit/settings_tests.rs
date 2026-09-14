use super::*;

// Shared with the other keepbook-server test modules; each one loads its own
// copy of the file.
#[allow(clippy::duplicate_mod)]
#[path = "test_support.rs"]
mod test_support;

use test_support::{remove_test_config, unique_test_config_path, write_test_config};

#[test]
fn managed_start_minimized_values_parse_common_boolean_forms() -> Result<()> {
    assert!(parse_bool_setting("true")?);
    assert!(parse_bool_setting("1")?);
    assert!(!parse_bool_setting("off")?);
    assert!(parse_bool_setting("sometimes").is_err());
    Ok(())
}

#[test]
fn desktop_start_minimized_to_tray_reads_tray_config() -> Result<()> {
    let config_path = unique_test_config_path("desktop-start-minimized");
    write_test_config(&config_path, "[tray]\nstart_minimized = true\n")?;

    assert!(desktop_start_minimized_to_tray(&config_path)?);
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn desktop_window_decorations_reads_tray_config() -> Result<()> {
    let config_path = unique_test_config_path("desktop-window-decorations");
    write_test_config(&config_path, "[tray]\nwindow_decorations = \"system\"\n")?;

    assert_eq!(
        desktop_window_decorations(&config_path)?,
        WindowDecorationsConfig::System
    );
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn write_application_settings_updates_tray_config_without_replacing_other_settings() -> Result<()> {
    let config_path = unique_test_config_path("application-settings");
    write_test_config(
        &config_path,
        "reporting_currency = \"EUR\"\n\n[tray]\nhistory_points = 12\n",
    )?;

    write_application_settings(
        &config_path,
        &ApplicationSettingsInput {
            start_minimized_to_tray: true,
            window_decorations: "hidden".to_string(),
        },
    )?;

    assert!(desktop_start_minimized_to_tray(&config_path)?);
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("reporting_currency = \"EUR\""));
    assert!(content.contains("history_points = 12"));
    assert!(content.contains("start_minimized = true"));
    assert!(content.contains("window_decorations = \"hidden\""));
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn write_application_settings_rejects_unknown_window_decorations() -> Result<()> {
    let config_path = unique_test_config_path("invalid-window-decorations");
    let error = write_application_settings(
        &config_path,
        &ApplicationSettingsInput {
            start_minimized_to_tray: false,
            window_decorations: "custom".to_string(),
        },
    )
    .expect_err("unknown window decoration values should fail");

    assert!(error
        .to_string()
        .contains("unsupported window decorations setting"));
    assert!(!config_path.exists());
    Ok(())
}
