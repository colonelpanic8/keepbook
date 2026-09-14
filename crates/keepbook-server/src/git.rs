use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use keepbook::repositories;
use toml_edit::{value, Item, Table};

use crate::dto::{
    GitRemoteSettings, GitRepoState, GitSettingsInput, GitSyncCancelToken, GitSyncInput,
};
use crate::settings::{load_config_doc, non_empty};

const DEFAULT_SSH_IDENTITY_FILES: &[&str] = &[
    "id_ed25519",
    "id_rsa",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_ed25519_sk",
    "keepbook_sync_key",
];

pub(crate) fn load_git_remote_settings(config_path: &Path) -> Result<GitRemoteSettings> {
    let device_config_path = keepbook::config::device_config_path(config_path);
    load_git_remote_settings_from(config_path, device_config_path.as_deref())
}

fn load_git_remote_settings_from(
    config_path: &Path,
    device_config_path: Option<&Path>,
) -> Result<GitRemoteSettings> {
    let doc = load_config_doc(config_path)?;
    let defaults = GitRemoteSettings::default();
    let git_sync = doc.get("git_sync");
    let ssh_key_path = device_config_path
        .map(keepbook::config::load_device_ssh_key_path_from)
        .transpose()?
        .flatten()
        .map(|path| path.display().to_string());
    Ok(GitRemoteSettings {
        host: table_string(git_sync, "host").unwrap_or(defaults.host),
        repo: table_string(git_sync, "repo").unwrap_or(defaults.repo),
        branch: table_string(git_sync, "branch").unwrap_or(defaults.branch),
        ssh_user: table_string(git_sync, "ssh_user").unwrap_or(defaults.ssh_user),
        ssh_key_path,
    })
}

pub(crate) fn with_default_desktop_ssh_key_path(
    config_path: &Path,
    mut settings: GitRemoteSettings,
) -> GitRemoteSettings {
    if let Some(path) = settings
        .ssh_key_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let resolved = resolve_config_relative_path(config_path, path);
        if resolved.is_file() {
            settings.ssh_key_path = Some(resolved.display().to_string());
            return settings;
        }
        settings.ssh_key_path = None;
    }

    if let Some(path) = default_desktop_ssh_key_path(config_path) {
        settings.ssh_key_path = Some(path.display().to_string());
    }

    settings
}

fn default_desktop_ssh_key_path(config_path: &Path) -> Option<PathBuf> {
    let saved_key_path = default_git_ssh_key_path(config_path).ok();
    let home_dir = if cfg!(target_os = "android") {
        None
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    };
    select_default_ssh_key_path(saved_key_path, home_dir.as_deref())
}

fn select_default_ssh_key_path(
    saved_key_path: Option<PathBuf>,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    saved_key_path
        .filter(|path| path.is_file())
        .or_else(|| home_dir.and_then(default_ssh_key_path_in_home))
}

fn default_ssh_key_path_in_home(home_dir: &Path) -> Option<PathBuf> {
    let ssh_dir = home_dir.join(".ssh");
    DEFAULT_SSH_IDENTITY_FILES
        .iter()
        .map(|name| ssh_dir.join(name))
        .find(|path| path.is_file())
}

fn table_string(table: Option<&Item>, key: &str) -> Option<String> {
    table?
        .as_table_like()?
        .get(key)?
        .as_str()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn write_git_settings(config_path: &Path, input: &GitSettingsInput) -> Result<()> {
    let device_config_path = keepbook::config::device_config_path(config_path)
        .context("cannot resolve device-local keepbook config path")?;
    write_git_settings_to(config_path, &device_config_path, input)
}

fn write_git_settings_to(
    config_path: &Path,
    device_config_path: &Path,
    input: &GitSettingsInput,
) -> Result<()> {
    let mut doc = load_config_doc(config_path)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    doc["data_dir"] = value(portable_data_dir_value(config_path, input.data_dir.trim()));
    if doc
        .get("git_sync")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        doc.insert("git_sync", Item::Table(Table::new()));
    }
    doc["git_sync"]["host"] = value(non_empty(input.host.trim(), "github.com"));
    doc["git_sync"]["repo"] = value(input.repo.trim());
    doc["git_sync"]["branch"] = value(non_empty(input.branch.trim(), "master"));
    doc["git_sync"]["ssh_user"] = value(non_empty(input.ssh_user.trim(), "git"));
    if let Some(git_sync) = doc["git_sync"].as_table_like_mut() {
        git_sync.remove("ssh_key_path");
    }
    if let Some(git) = doc.get_mut("git").and_then(Item::as_table_like_mut) {
        git.remove("ssh_key_path");
    }

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    write_device_ssh_key_path(device_config_path, input.ssh_key_path.as_deref())?;
    Ok(())
}

fn write_device_ssh_key_path(device_config_path: &Path, ssh_key_path: Option<&str>) -> Result<()> {
    let ssh_key_path = ssh_key_path.map(str::trim).filter(|path| !path.is_empty());
    if ssh_key_path.is_none() && !device_config_path.exists() {
        return Ok(());
    }

    let mut doc = load_config_doc(device_config_path)?;
    if doc
        .get("git")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        doc.insert("git", Item::Table(Table::new()));
    }
    if let Some(path) = ssh_key_path {
        doc["git"]["ssh_key_path"] = value(path);
    } else if let Some(git) = doc["git"].as_table_like_mut() {
        git.remove("ssh_key_path");
    }

    if let Some(parent) = device_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(device_config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", device_config_path.display()))?;
    Ok(())
}

pub(crate) fn resolve_input_data_dir(config_path: &Path, data_dir: &str) -> PathBuf {
    let path = PathBuf::from(data_dir);
    if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(path.clone()))
            .unwrap_or(path)
    }
}

pub(crate) fn validate_git_data_dir(data_dir: &Path) -> Result<()> {
    if data_dir.parent().is_none() {
        anyhow::bail!(
            "Git data directory cannot be a filesystem root: {}",
            data_dir.display()
        );
    }

    Ok(())
}

pub(crate) fn read_git_repo_state(data_dir: &Path) -> GitRepoState {
    let state = repositories::read_git_state(data_dir);
    GitRepoState {
        cloned: state.cloned,
        remote_url: state.remote_url,
        branch: state.branch,
        commit: state.commit,
    }
}

pub(crate) fn prepare_git_ssh_environment(config_path: &Path) -> Result<()> {
    let Some(state_dir) = private_state_dir(config_path) else {
        return Ok(());
    };

    configure_git_ssh_home(&state_dir)?;

    let ssh_dir = state_dir.join(".ssh");
    std::fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed to create {}", ssh_dir.display()))?;

    let known_hosts = ssh_dir.join("known_hosts");
    if !known_hosts.exists() {
        std::fs::write(&known_hosts, "")
            .with_context(|| format!("failed to create {}", known_hosts.display()))?;
    }

    Ok(())
}

fn configure_git_ssh_home(state_dir: &Path) -> Result<()> {
    if cfg!(target_os = "android") || std::env::var_os("HOME").is_none() {
        std::env::set_var("HOME", state_dir);
    }

    set_libgit2_homedir(state_dir)
}

#[cfg(target_os = "android")]
fn set_libgit2_homedir(state_dir: &Path) -> Result<()> {
    use std::ffi::CString;

    let path = CString::new(state_dir.to_string_lossy().as_bytes())
        .with_context(|| format!("failed to encode libgit2 home {}", state_dir.display()))?;
    unsafe {
        libgit2_sys::git_libgit2_init();
        let status = libgit2_sys::git_libgit2_opts(
            libgit2_sys::GIT_OPT_SET_HOMEDIR as std::os::raw::c_int,
            path.as_ptr(),
        );
        if status < 0 {
            anyhow::bail!("failed to set libgit2 home to {}", state_dir.display());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn set_libgit2_homedir(_state_dir: &Path) -> Result<()> {
    Ok(())
}

fn default_git_ssh_key_path(config_path: &Path) -> Result<PathBuf> {
    let Some(state_dir) = private_state_dir(config_path) else {
        anyhow::bail!("cannot resolve SSH key path without a config directory");
    };

    Ok(state_dir.join(".ssh").join("keepbook_sync_key"))
}

fn private_state_dir(_config_path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        app_private_state_dir(_config_path)
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .map(|state_dir| state_dir.join("keepbook"))
    }
}

#[cfg(any(target_os = "android", test))]
fn app_private_state_dir(config_path: &Path) -> Option<PathBuf> {
    let config_dir = config_path.parent()?;
    Some(
        config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config_dir.to_path_buf()),
    )
}

pub(crate) fn persist_git_private_key(config_path: &Path, private_key_pem: &str) -> Result<String> {
    let key_path = default_git_ssh_key_path(config_path)?;
    let Some(parent) = key_path.parent() else {
        anyhow::bail!(
            "cannot resolve SSH key directory for {}",
            key_path.display()
        );
    };

    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&key_path)
            .with_context(|| format!("failed to open {}", key_path.display()))?;
        file.write_all(private_key_pem.trim_end().as_bytes())
            .with_context(|| format!("failed to write {}", key_path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to finalize {}", key_path.display()))?;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", key_path.display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(&key_path, format!("{}\n", private_key_pem.trim_end()))
            .with_context(|| format!("failed to write {}", key_path.display()))?;
    }

    Ok(key_path.display().to_string())
}

pub(crate) fn resolve_git_private_key(
    config_path: &Path,
    settings: &GitRemoteSettings,
    input: &GitSyncInput,
) -> Result<String> {
    let inline_key = input.private_key_pem.trim();
    if !inline_key.is_empty() {
        return Ok(input.private_key_pem.clone());
    }

    let Some(key_path) = settings.ssh_key_path.as_deref() else {
        anyhow::bail!("SSH private key is empty and no saved SSH key path is configured");
    };

    let key_path = resolve_config_relative_path(config_path, key_path);
    std::fs::read_to_string(&key_path)
        .with_context(|| format!("failed to read saved SSH key {}", key_path.display()))
        .and_then(|contents| {
            if contents.trim().is_empty() {
                anyhow::bail!("saved SSH key {} is empty", key_path.display());
            }
            Ok(contents)
        })
}

pub(crate) fn activate_age_identity_from_git_settings(config_path: &Path) -> Result<()> {
    if let Ok(default_key_path) = default_git_ssh_key_path(config_path) {
        if default_key_path.is_file() {
            std::env::set_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH", default_key_path);
            return Ok(());
        }
    }

    let settings =
        with_default_desktop_ssh_key_path(config_path, load_git_remote_settings(config_path)?);
    let Some(ssh_key_path) = settings
        .ssh_key_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Ok(());
    };

    let resolved = resolve_config_relative_path(config_path, ssh_key_path);
    std::env::set_var("KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH", resolved);
    Ok(())
}

fn resolve_config_relative_path(config_path: &Path, path: &str) -> PathBuf {
    let path = expand_home_path(path);
    if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(path.clone()))
            .unwrap_or(path)
    }
}

fn expand_home_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(path)
}

fn portable_data_dir_value(config_path: &Path, data_dir: &str) -> String {
    let resolved = normalize_path_components(resolve_config_relative_path(config_path, data_dir));
    if let Some(config_dir) = config_path.parent() {
        if resolved == normalize_path_components(config_dir.to_path_buf()) {
            return ".".to_string();
        }
    }

    if Path::new(data_dir).is_absolute() {
        portable_path_value(&resolved)
    } else {
        data_dir.to_string()
    }
}

fn portable_path_value(path: &Path) -> String {
    home_relative_path(path)
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn home_relative_path(path: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let path = normalize_path_components(path.to_path_buf());
    let home = normalize_path_components(home);
    let relative = path.strip_prefix(home).ok()?;
    if relative.as_os_str().is_empty() {
        Some(PathBuf::from("~"))
    } else {
        Some(PathBuf::from("~").join(relative))
    }
}

fn normalize_path_components(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

fn log_git_sync_event(message: impl AsRef<str>) {
    let message = message.as_ref();
    tracing::info!("{message}");

    #[cfg(target_os = "android")]
    eprintln!("{message}");
}

fn normalize_repo_path(repo: &str) -> String {
    let repo = repo.trim();
    if repo.ends_with(".git") {
        repo.to_string()
    } else {
        format!("{repo}.git")
    }
}

pub(crate) fn build_ssh_remote_url(host: &str, repo: &str, ssh_user: &str) -> String {
    let repo = repo.trim();
    if is_explicit_git_remote(repo) {
        return normalize_remote_url_for_ssh(repo, ssh_user);
    }

    let repo = normalize_repo_path(repo);
    let host = host.trim();
    let ssh_user = non_empty(ssh_user.trim(), "git");

    if host.contains("://") || host.contains(':') {
        let host = host.strip_prefix("ssh://").unwrap_or(host);
        format!("ssh://{ssh_user}@{host}/{repo}")
    } else {
        format!("{ssh_user}@{host}:{repo}")
    }
}

fn is_explicit_git_remote(remote: &str) -> bool {
    remote.contains("://") || (remote.contains('@') && remote.contains(':'))
}

pub(crate) fn normalize_remote_url_for_ssh(remote_url: &str, ssh_user: &str) -> String {
    https_remote_to_ssh(remote_url, ssh_user).unwrap_or_else(|| remote_url.trim().to_string())
}

fn https_remote_to_ssh(remote_url: &str, ssh_user: &str) -> Option<String> {
    let remote_url = remote_url.trim();
    let rest = remote_url
        .strip_prefix("https://")
        .or_else(|| remote_url.strip_prefix("http://"))?;
    let (host, repo) = rest.split_once('/')?;
    let host = host.rsplit('@').next()?.trim();
    let repo = repo
        .split(['?', '#'])
        .next()
        .unwrap_or(repo)
        .trim_matches('/');

    if host.is_empty() || repo.is_empty() {
        return None;
    }

    let ssh_user = non_empty(ssh_user.trim(), "git");
    let repo = normalize_repo_path(repo);
    if host.contains(':') {
        Some(format!("ssh://{ssh_user}@{host}/{repo}"))
    } else {
        Some(format!("{ssh_user}@{host}:{repo}"))
    }
}

pub(crate) fn sync_git_ssh(
    data_dir: &Path,
    remote_url: &str,
    branch: &str,
    private_key_pem: &str,
    cancel_token: &GitSyncCancelToken,
) -> Result<()> {
    use git2::{build::CheckoutBuilder, build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks};
    use git2::{Repository, ResetType};

    fn fetch_options(
        ssh_user: &str,
        private_key_pem: &str,
        cancel_token: &GitSyncCancelToken,
    ) -> FetchOptions<'static> {
        let ssh_user = ssh_user.to_string();
        let private_key_pem = private_key_pem.to_string();
        let transfer_cancel = cancel_token.clone();
        let sideband_cancel = cancel_token.clone();
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |_url, username_from_url, _allowed| {
            let username = username_from_url.unwrap_or(&ssh_user);
            Cred::ssh_key_from_memory(username, None, &private_key_pem, None)
        });
        callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));
        callbacks.transfer_progress(move |_progress| !transfer_cancel.is_cancelled());
        callbacks.sideband_progress(move |_progress| !sideband_cancel.is_cancelled());

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);
        fetch_options
    }

    fn with_cancel_context<T>(
        result: std::result::Result<T, git2::Error>,
        cancel_token: &GitSyncCancelToken,
        context: impl std::fmt::Display,
    ) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(_error) if cancel_token.is_cancelled() => {
                anyhow::bail!("Git sync cancelled");
            }
            Err(error) => Err(error).with_context(|| context.to_string()),
        }
    }

    cancel_token.check()?;

    let ssh_user = remote_url
        .split('@')
        .next()
        .and_then(|prefix| prefix.rsplit(['/', ':']).next())
        .filter(|value| !value.is_empty())
        .unwrap_or("git")
        .to_string();

    let is_existing_repo = data_dir.join(".git").exists();
    log_git_sync_event(format!(
        "Keepbook git sync {} {remote_url} branch {branch} in {}",
        if is_existing_repo {
            "fetching"
        } else {
            "cloning"
        },
        data_dir.display()
    ));

    let repo = if is_existing_repo {
        let repo = Repository::open(data_dir)
            .with_context(|| format!("failed to open git repository {}", data_dir.display()))?;
        match repo.find_remote("origin") {
            Ok(remote) => {
                if remote.url() != Some(remote_url) {
                    repo.remote_set_url("origin", remote_url)?;
                }
            }
            Err(_) => {
                repo.remote("origin", remote_url)?;
            }
        }
        repo
    } else {
        if let Some(parent) = data_dir.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut builder = RepoBuilder::new();
        builder.branch(branch);
        builder.fetch_options(fetch_options(&ssh_user, private_key_pem, cancel_token));
        with_cancel_context(
            builder.clone(remote_url, data_dir),
            cancel_token,
            format!("failed to clone {remote_url} into {}", data_dir.display()),
        )?
    };
    cancel_token.check()?;

    {
        let mut remote = repo.find_remote("origin")?;
        let mut options = fetch_options(&ssh_user, private_key_pem, cancel_token);
        let refspec = format!("refs/heads/{branch}:refs/remotes/origin/{branch}");
        with_cancel_context(
            remote.fetch(&[refspec.as_str()], Some(&mut options), None),
            cancel_token,
            format!("failed to fetch origin/{branch}"),
        )?;
    }
    cancel_token.check()?;

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let obj = repo
        .revparse_single(&remote_ref)
        .with_context(|| format!("failed to resolve {remote_ref}"))?;
    let commit = obj.peel_to_commit()?;
    let local_ref = format!("refs/heads/{branch}");
    if repo.find_reference(&local_ref).is_err() {
        repo.branch(branch, &commit, true)?;
    }
    cancel_token.check()?;
    repo.set_head(&local_ref)?;
    repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
    repo.reset(commit.as_object(), ResetType::Hard, None)?;
    log_git_sync_event(format!(
        "Keepbook git sync checked out {branch} at {} in {}",
        commit.id(),
        data_dir.display()
    ));

    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/git_tests.rs"]
mod git_tests;
