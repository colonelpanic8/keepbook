use super::*;

// The keepbook crate's unit tests share this guard; each test binary is its own
// process, so each gets its own lock.
#[path = "../../../../tests/unit/env_guard.rs"]
mod env_guard;

use env_guard::EnvGuard;

// Shared with the other keepbook-server test modules; each one loads its own
// copy of the file.
#[allow(clippy::duplicate_mod)]
#[path = "test_support.rs"]
mod test_support;

use test_support::{remove_test_config, unique_test_config_path, write_test_config};

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
    let device_config_path = config_path.with_file_name("device.toml");
    write_test_config(&config_path, "git_sync = \"invalid\"\n")?;

    let settings = load_git_remote_settings_from(&config_path, Some(&device_config_path))?;
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
    let device_config_path = config_path.with_file_name("device.toml");
    write_test_config(&config_path, "data_dir = \"./old-data\"\n")?;

    write_git_settings_to(
        &config_path,
        &device_config_path,
        &GitSettingsInput {
            data_dir: "/tmp/keepbook-data".to_string(),
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
            ssh_key_path: Some(".ssh/keepbook_sync_key".to_string()),
        },
    )?;

    let settings = load_git_remote_settings_from(&config_path, Some(&device_config_path))?;
    assert_eq!(settings.host, "github.com");
    assert_eq!(settings.repo, "colonelpanic8/keepbook-data");
    assert_eq!(settings.branch, "master");
    assert_eq!(settings.ssh_user, "git");
    assert_eq!(
        settings.ssh_key_path.as_deref(),
        Some(
            device_config_path
                .parent()
                .expect("device config should have a parent")
                .join(".ssh/keepbook_sync_key")
                .to_str()
                .expect("test path should be UTF-8")
        )
    );
    let content = std::fs::read_to_string(&config_path)?;
    assert!(content.contains("[git_sync]"));
    assert!(!content.contains("ssh_key_path"));
    let device_content = std::fs::read_to_string(&device_config_path)?;
    assert!(device_content.contains("[git]"));
    assert!(device_content.contains("ssh_key_path = \".ssh/keepbook_sync_key\""));
    remove_test_config(config_path);
    Ok(())
}

#[test]
fn write_git_settings_keeps_data_dir_portable_and_moves_ssh_key_path_to_device_config() -> Result<()>
{
    let config_path = unique_test_config_path("write-git-portable-paths");
    let device_config_path = config_path.with_file_name("device.toml");
    let config_dir = config_path
        .parent()
        .expect("test config should have parent")
        .to_path_buf();
    write_test_config(
        &config_path,
        "data_dir = \"./old-data\"\n[git_sync]\nssh_key_path = \"/Users/kat/.ssh/id_rsa\"\n[git]\nssh_key_path = \"/Users/kat/.ssh/id_ed25519\"\n",
    )?;

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.join("home"));
    let ssh_key_path = home.join(".ssh").join("id_ed25519");

    write_git_settings_to(
        &config_path,
        &device_config_path,
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
    assert!(!content.contains("ssh_key_path"));
    let device_content = std::fs::read_to_string(&device_config_path)?;
    assert!(device_content.contains("[git]"));
    assert!(device_content.contains(&format!("ssh_key_path = \"{}\"", ssh_key_path.display())));
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
fn saved_app_private_key_is_used_without_home_fallback() -> Result<()> {
    let config_path = unique_test_config_path("saved-app-private-key");
    let saved_key_path = config_path
        .parent()
        .expect("test config should have a parent")
        .join("keepbook_sync_key");
    std::fs::create_dir_all(
        saved_key_path
            .parent()
            .expect("test key should have a parent"),
    )?;
    std::fs::write(&saved_key_path, "test key")?;

    assert_eq!(
        select_default_ssh_key_path(Some(saved_key_path.clone()), None).as_deref(),
        Some(saved_key_path.as_path())
    );

    remove_test_config(config_path);
    Ok(())
}

#[test]
fn android_private_state_is_outside_data_repo() {
    let config_path = Path::new(
        "/data/user/0/org.colonelpanic.keepbook.dioxus/files/keepbook-data/keepbook.toml",
    );

    assert_eq!(
        app_private_state_dir(config_path).as_deref(),
        Some(Path::new(
            "/data/user/0/org.colonelpanic.keepbook.dioxus/files"
        ))
    );
}

#[test]
fn prepare_git_ssh_environment_creates_known_hosts_and_home_when_missing() -> Result<()> {
    let mut env = EnvGuard::new();
    let config_path = unique_test_config_path("prepare-git-ssh-env");
    write_test_config(&config_path, "data_dir = \".\"\n")?;

    let state_home = config_path
        .parent()
        .expect("test config should have parent")
        .join("state");
    let expected_state_dir = state_home.join("keepbook");
    env.remove("HOME");
    env.set("XDG_STATE_HOME", &state_home);

    let result = prepare_git_ssh_environment(&config_path);
    let home_after_prepare = std::env::var_os("HOME");

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
    let mut env = EnvGuard::new();
    let config_path = unique_test_config_path("age-identity-saved-key");
    write_test_config(&config_path, "data_dir = \".\"\n")?;

    let state_home = config_path
        .parent()
        .expect("test config should have parent")
        .join("state");
    env.set("XDG_STATE_HOME", &state_home);
    env.remove("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH");

    let key_path = default_git_ssh_key_path(&config_path)?;
    std::fs::create_dir_all(key_path.parent().expect("key path should have parent"))?;
    std::fs::write(&key_path, "fake test key")?;

    activate_age_identity_from_git_settings(&config_path)?;
    assert_eq!(
        std::env::var_os("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH").as_deref(),
        Some(key_path.as_os_str())
    );

    remove_test_config(config_path);
    Ok(())
}
