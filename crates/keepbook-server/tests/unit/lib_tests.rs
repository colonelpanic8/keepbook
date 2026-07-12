use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
#[test]
fn validate_git_data_dir_rejects_filesystem_root() {
    let error = validate_git_data_dir(Path::new("/")).expect_err("root should be rejected");
    assert!(error.to_string().contains("filesystem root"));
}

#[test]
fn validate_git_data_dir_accepts_nested_path() {
    validate_git_data_dir(Path::new("/tmp/keepbook-data")).expect("nested path should be valid");
}

#[test]
fn load_git_remote_settings_ignores_non_table_git_sync() -> Result<()> {
    let config_path = unique_test_config_path("load-git-non-table");
    write_test_config(&config_path, "git_sync = \"invalid\"\n")?;

    let settings = load_git_remote_settings(&config_path)?;
    let defaults = GitRemoteSettings::default();

    assert_eq!(settings.host, defaults.host);
    assert_eq!(settings.repo, defaults.repo);
    assert_eq!(settings.branch, defaults.branch);
    assert_eq!(settings.ssh_user, defaults.ssh_user);
    assert_eq!(settings.ssh_key_path, defaults.ssh_key_path);
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn write_git_settings_creates_missing_git_sync_table() -> Result<()> {
    let config_path = unique_test_config_path("write-git-missing-table");
    write_test_config(&config_path, "data_dir = \"./old-data\"\n")?;

    write_git_settings(
        &config_path,
        &GitSettingsInput {
            data_dir: "/tmp/keepbook-data".to_string(),
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
            ssh_key_path: Some(".ssh/keepbook_sync_key".to_string()),
        },
    )?;

    let settings = load_git_remote_settings(&config_path)?;
    assert_eq!(settings.host, "github.com");
    assert_eq!(settings.repo, "colonelpanic8/keepbook-data");
    assert_eq!(settings.branch, "master");
    assert_eq!(settings.ssh_user, "git");
    assert_eq!(
        settings.ssh_key_path.as_deref(),
        Some(".ssh/keepbook_sync_key")
    );
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("[git_sync]"));
    assert!(content.contains("[git]"));
    assert!(content.contains("ssh_key_path = \".ssh/keepbook_sync_key\""));
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn write_git_settings_keeps_data_dir_portable_and_writes_git_ssh_key_path() -> Result<()> {
    let config_path = unique_test_config_path("write-git-portable-paths");
    let config_dir = config_path
        .parent()
        .expect("test config should have parent")
        .to_path_buf();
    write_test_config(
        &config_path,
        "data_dir = \"./old-data\"\n[git_sync]\nssh_key_path = \"/Users/kat/.ssh/id_ed25519\"\n",
    )?;

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.join("home"));
    let ssh_key_path = home.join(".ssh").join("id_ed25519");

    write_git_settings(
        &config_path,
        &GitSettingsInput {
            data_dir: config_dir.display().to_string(),
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
            ssh_key_path: Some(ssh_key_path.display().to_string()),
        },
    )?;

    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("data_dir = \".\""));
    assert!(content.contains("[git]"));
    assert!(content.contains(&format!("ssh_key_path = \"{}\"", ssh_key_path.display())));
    assert!(!content.contains("[git_sync]\nssh_key_path"));
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn build_ssh_remote_url_converts_explicit_https_github_remote() {
    assert_eq!(
        build_ssh_remote_url(
            "github.com",
            "https://github.com/colonelpanic8/keepbook-data.git",
            "git",
        ),
        "git@github.com:colonelpanic8/keepbook-data.git"
    );
}

#[test]
fn normalize_remote_url_for_ssh_converts_existing_https_origin() {
    assert_eq!(
        normalize_remote_url_for_ssh("https://github.com/colonelpanic8/keepbook-data.git", "git",),
        "git@github.com:colonelpanic8/keepbook-data.git"
    );
}

#[test]
fn normalize_remote_url_for_ssh_leaves_ssh_origin_unchanged() {
    assert_eq!(
        normalize_remote_url_for_ssh("git@github.com:colonelpanic8/keepbook-data.git", "git"),
        "git@github.com:colonelpanic8/keepbook-data.git"
    );
}

#[test]
fn default_ssh_key_path_prefers_ed25519_then_rsa() -> Result<()> {
    let home = unique_test_config_path("default-ssh-key-order")
        .parent()
        .expect("test config should have parent")
        .join("home");
    let ssh_dir = home.join(".ssh");
    std::fs::create_dir_all(&ssh_dir)?;
    std::fs::write(ssh_dir.join("id_rsa"), "rsa")?;
    std::fs::write(ssh_dir.join("id_ed25519"), "ed25519")?;

    let expected = ssh_dir.join("id_ed25519");
    assert_eq!(
        default_ssh_key_path_in_home(&home).as_deref(),
        Some(expected.as_path())
    );
    let _ = std::fs::remove_dir_all(home.parent().unwrap_or(&home));
    Ok(())
}

#[test]
fn prepare_git_ssh_environment_creates_known_hosts_and_home_when_missing() -> Result<()> {
    let config_path = unique_test_config_path("prepare-git-ssh-env");
    write_test_config(&config_path, "data_dir = \".\"\n")?;

    let state_home = config_path
        .parent()
        .expect("test config should have parent")
        .join("state");
    let expected_state_dir = state_home.join("keepbook");
    let old_home = std::env::var_os("HOME");
    let old_state_home = std::env::var_os("XDG_STATE_HOME");
    std::env::remove_var("HOME");
    std::env::set_var("XDG_STATE_HOME", &state_home);

    let result = prepare_git_ssh_environment(&config_path);
    let home_after_prepare = std::env::var_os("HOME");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match old_state_home {
        Some(value) => std::env::set_var("XDG_STATE_HOME", value),
        None => std::env::remove_var("XDG_STATE_HOME"),
    }

    result?;
    assert_eq!(
        home_after_prepare.as_deref(),
        Some(expected_state_dir.as_os_str())
    );
    assert!(expected_state_dir
        .join(".ssh")
        .join("known_hosts")
        .is_file());

    remove_test_config(config_path);
    Ok(())
}

#[test]
fn configured_ssh_key_path_wins_over_default() {
    let config_path = unique_test_config_path("configured-ssh-key-wins");
    let expected = config_path
        .parent()
        .expect("test config should have parent")
        .join(".ssh/keepbook_sync_key");
    std::fs::create_dir_all(expected.parent().expect("test key should have parent"))
        .expect("test key parent should be created");
    std::fs::write(&expected, "test key").expect("test key should be written");
    let settings = with_default_desktop_ssh_key_path(
        &config_path,
        GitRemoteSettings {
            ssh_key_path: Some(".ssh/keepbook_sync_key".to_string()),
            ..GitRemoteSettings::default()
        },
    );

    assert_eq!(
        settings.ssh_key_path.as_deref(),
        Some(expected.to_str().expect("test path should be UTF-8"))
    );
    remove_test_config(config_path);
}

#[test]
fn missing_configured_ssh_key_path_is_not_returned() {
    let config_path = unique_test_config_path("missing-configured-ssh-key");
    let missing = config_path
        .parent()
        .expect("test config should have parent")
        .join(".ssh/missing_key");
    let missing = missing
        .to_str()
        .expect("test path should be UTF-8")
        .to_string();
    let settings = with_default_desktop_ssh_key_path(
        &config_path,
        GitRemoteSettings {
            ssh_key_path: Some(missing.clone()),
            ..GitRemoteSettings::default()
        },
    );

    assert_ne!(settings.ssh_key_path.as_deref(), Some(missing.as_str()));
    remove_test_config(config_path);
}

#[test]
fn activate_age_identity_prefers_saved_keepbook_sync_key() -> Result<()> {
    let config_path = unique_test_config_path("age-identity-saved-key");
    write_test_config(&config_path, "data_dir = \".\"\n")?;

    let state_home = config_path
        .parent()
        .expect("test config should have parent")
        .join("state");
    let old_state_home = std::env::var_os("XDG_STATE_HOME");
    let old_age_identity = std::env::var_os("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH");
    std::env::set_var("XDG_STATE_HOME", &state_home);
    std::env::remove_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH");

    let key_path = default_git_ssh_key_path(&config_path)?;
    std::fs::create_dir_all(key_path.parent().expect("key path should have parent"))?;
    std::fs::write(&key_path, "fake test key")?;

    activate_age_identity_from_git_settings(&config_path)?;
    assert_eq!(
        std::env::var_os("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH").as_deref(),
        Some(key_path.as_os_str())
    );

    match old_state_home {
        Some(value) => std::env::set_var("XDG_STATE_HOME", value),
        None => std::env::remove_var("XDG_STATE_HOME"),
    }
    match old_age_identity {
        Some(value) => std::env::set_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH", value),
        None => std::env::remove_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH"),
    }
    remove_test_config(config_path);
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
        },
    )?;

    assert!(desktop_start_minimized_to_tray(&config_path)?);
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("reporting_currency = \"EUR\""));
    assert!(content.contains("history_points = 12"));
    assert!(content.contains("start_minimized = true"));
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn account_portfolio_override_query_decodes_json_param() -> Result<()> {
    let encoded_overrides = serde_json::to_string(&serde_json::json!([
        {
            "account_id": "checking",
            "exclude_from_portfolio": false
        },
        {
            "account_id": "brokerage",
            "exclude_from_portfolio": true
        },
        {
            "account_id": "ira",
            "exclude_from_portfolio": true
        }
    ]))?;
    let encoded_query = serde_urlencoded::to_string([
        ("granularity", "weekly"),
        ("account_portfolio_overrides", encoded_overrides.as_str()),
    ])?;
    let query = serde_urlencoded::from_str::<HistoryQuery>(&encoded_query)?;

    assert_eq!(
        query.account_portfolio_overrides.as_deref(),
        Some(encoded_overrides.as_str())
    );

    let overrides = account_portfolio_exclusion_overrides(&query.account_portfolio_overrides)?;
    assert_eq!(overrides.get("checking"), Some(&false));
    assert_eq!(overrides.get("brokerage"), Some(&true));
    assert_eq!(overrides.get("ira"), Some(&true));
    Ok(())
}

fn unique_test_config_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "keepbook-server-{name}-{}-{nanos}/keepbook.toml",
        std::process::id()
    ))
}

fn write_test_config(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

fn remove_test_config(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}
