use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

use crate::config::{self, Config, GitConfig};

fn default_branch() -> String {
    "master".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RepositoryDeclaration {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub remote: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub config_path: PathBuf,
    pub remote: String,
    pub branch: String,
    pub managed: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RepositoryManifest {
    #[serde(alias = "active_repository")]
    default_repository: Option<String>,
    repositories: Vec<RepositoryDeclaration>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct DeviceFile {
    repositories: DeviceRepositories,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct DeviceRepositories {
    active_repository: Option<String>,
    local: Vec<RepositoryDeclaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryRegistry {
    pub manifest_path: PathBuf,
    pub device_config_path: PathBuf,
    pub default_repository: Option<String>,
    pub active_repository: Option<String>,
    pub repositories: Vec<RepositoryEntry>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositorySetupStatus {
    Cloned,
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RepositorySetupResult {
    pub id: String,
    pub name: String,
    pub path: String,
    pub status: RepositorySetupStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RepositorySetupOutput {
    pub ok: bool,
    pub manifest_path: String,
    pub repositories: Vec<RepositorySetupResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryGitState {
    pub cloned: bool,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

pub fn default_app_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("keepbook")
        .join("app.toml")
}

pub fn device_config_path(manifest_path: &Path) -> Result<PathBuf> {
    config::device_config_path(manifest_path)
        .context("cannot resolve the device-local Keepbook config path")
}

pub fn load_registry(manifest_path: &Path) -> Result<RepositoryRegistry> {
    let manifest_path = absolute_path(manifest_path)?;
    let device_config_path = device_config_path(&manifest_path)?;
    load_registry_from_paths(&manifest_path, &device_config_path)
}

fn load_registry_from_paths(
    manifest_path: &Path,
    device_config_path: &Path,
) -> Result<RepositoryRegistry> {
    let manifest_path = absolute_path(manifest_path)?;
    let device_config_path = absolute_path(device_config_path)?;
    let manifest = load_manifest_if_present(&manifest_path)?;
    let device = load_device_file(&device_config_path)?;
    let manifest_base = manifest_path.parent().unwrap_or(Path::new("."));
    let device_base = device_config_path.parent().unwrap_or(Path::new("."));

    let mut repositories = Vec::new();
    for declaration in &manifest.repositories {
        merge_entry(
            &mut repositories,
            resolve_declaration(declaration, manifest_base, true)?,
        )?;
    }
    for declaration in &device.repositories.local {
        merge_entry(
            &mut repositories,
            resolve_declaration(declaration, device_base, false)?,
        )?;
    }

    let active_repository = device
        .repositories
        .active_repository
        .filter(|id| repositories.iter().any(|entry| &entry.id == id))
        .or_else(|| {
            manifest
                .default_repository
                .clone()
                .filter(|id| repositories.iter().any(|entry| &entry.id == id))
        })
        .or_else(|| repositories.first().map(|entry| entry.id.clone()));

    Ok(RepositoryRegistry {
        manifest_path,
        device_config_path,
        default_repository: manifest.default_repository,
        active_repository,
        repositories,
    })
}

pub fn active_repository_config_path(
    manifest_path: &Path,
    fallback: impl AsRef<Path>,
) -> Result<PathBuf> {
    let registry = load_registry(manifest_path)?;
    let config_path = registry.active_repository.as_deref().and_then(|active| {
        registry
            .repositories
            .iter()
            .find(|entry| entry.id == active)
            .map(|entry| entry.config_path.clone())
            .filter(|path| path.is_file())
    });
    Ok(config_path.unwrap_or_else(|| fallback.as_ref().to_path_buf()))
}

pub fn add_local_repository(
    manifest_path: &Path,
    declaration: RepositoryDeclaration,
) -> Result<RepositoryRegistry> {
    let manifest_path = absolute_path(manifest_path)?;
    let device_config_path = device_config_path(&manifest_path)?;
    add_local_repository_from_paths(&manifest_path, &device_config_path, declaration)
}

fn add_local_repository_from_paths(
    manifest_path: &Path,
    device_config_path: &Path,
    declaration: RepositoryDeclaration,
) -> Result<RepositoryRegistry> {
    let registry = load_registry_from_paths(manifest_path, device_config_path)?;
    let device_base = registry
        .device_config_path
        .parent()
        .unwrap_or(Path::new("."));
    let candidate = resolve_declaration(&declaration, device_base, false)?;
    anyhow::ensure!(
        !registry
            .repositories
            .iter()
            .any(|entry| entry.id == candidate.id),
        "A repository with id {} is already registered",
        candidate.id
    );
    anyhow::ensure!(
        !registry
            .repositories
            .iter()
            .any(|entry| entry.path == candidate.path),
        "A repository at {} is already registered",
        candidate.path.display()
    );

    let mut device = load_device_file(&registry.device_config_path)?;
    device
        .repositories
        .local
        .push(declaration_from_entry(&candidate));
    save_device_repositories(&registry.device_config_path, &device.repositories)?;
    load_registry_from_paths(manifest_path, device_config_path)
}

pub fn add_resolved_local_repository(
    manifest_path: &Path,
    entry: &RepositoryEntry,
) -> Result<RepositoryRegistry> {
    add_local_repository(manifest_path, declaration_from_entry(entry))
}

pub fn set_active_repository(
    manifest_path: &Path,
    repository_id: &str,
) -> Result<RepositoryRegistry> {
    let manifest_path = absolute_path(manifest_path)?;
    let device_config_path = device_config_path(&manifest_path)?;
    set_active_repository_from_paths(&manifest_path, &device_config_path, repository_id)
}

fn set_active_repository_from_paths(
    manifest_path: &Path,
    device_config_path: &Path,
    repository_id: &str,
) -> Result<RepositoryRegistry> {
    let registry = load_registry_from_paths(manifest_path, device_config_path)?;
    anyhow::ensure!(
        registry
            .repositories
            .iter()
            .any(|entry| entry.id == repository_id),
        "Unknown Keepbook repository: {repository_id}"
    );
    let mut device = load_device_file(&registry.device_config_path)?;
    device.repositories.active_repository = Some(repository_id.to_string());
    save_device_repositories(&registry.device_config_path, &device.repositories)?;
    load_registry_from_paths(manifest_path, device_config_path)
}

pub fn remove_local_repository(
    manifest_path: &Path,
    repository_id: &str,
) -> Result<RepositoryRegistry> {
    let manifest_path = absolute_path(manifest_path)?;
    let device_config_path = device_config_path(&manifest_path)?;
    remove_local_repository_from_paths(&manifest_path, &device_config_path, repository_id)
}

fn remove_local_repository_from_paths(
    manifest_path: &Path,
    device_config_path: &Path,
    repository_id: &str,
) -> Result<RepositoryRegistry> {
    let registry = load_registry_from_paths(manifest_path, device_config_path)?;
    let entry = registry
        .repositories
        .iter()
        .find(|entry| entry.id == repository_id)
        .with_context(|| format!("Unknown Keepbook repository: {repository_id}"))?;
    anyhow::ensure!(
        !entry.managed,
        "Repository {} is managed by {} and cannot be removed from the app",
        entry.name,
        registry.manifest_path.display()
    );
    anyhow::ensure!(
        registry.active_repository.as_deref() != Some(repository_id),
        "Switch to another repository before removing the active repository"
    );

    let mut device = load_device_file(&registry.device_config_path)?;
    let previous_len = device.repositories.local.len();
    device
        .repositories
        .local
        .retain(|entry| entry.id != repository_id);
    anyhow::ensure!(
        device.repositories.local.len() != previous_len,
        "Unknown device-local Keepbook repository: {repository_id}"
    );
    save_device_repositories(&registry.device_config_path, &device.repositories)?;
    load_registry_from_paths(manifest_path, device_config_path)
}

pub fn unique_repository_id(name: &str, repositories: &[RepositoryEntry]) -> String {
    let base = repository_id_slug(name);
    if !repositories.iter().any(|entry| entry.id == base) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !repositories.iter().any(|entry| entry.id == *candidate))
        .expect("repository id suffixes are unbounded")
}

pub fn absolute_repository_path(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    anyhow::ensure!(!trimmed.is_empty(), "Repository location is required");
    let path = expand_tilde(Path::new(trimmed));
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .context("cannot resolve the current directory")?
            .join(path)
    };
    let normalized = normalize_path(absolute);
    validate_checkout_path(&normalized)?;
    Ok(normalized)
}

pub fn repository_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Keepbook")
        .to_string()
}

pub fn read_git_state(path: &Path) -> RepositoryGitState {
    let Ok(repo) = Repository::open(path) else {
        return RepositoryGitState {
            cloned: false,
            remote_url: None,
            branch: None,
            commit: None,
        };
    };
    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().ok().map(|url| url.to_string()));
    let head = repo.head().ok();
    RepositoryGitState {
        cloned: true,
        remote_url,
        branch: head
            .as_ref()
            .filter(|head| head.is_branch())
            .and_then(|head| head.shorthand().ok().map(|branch| branch.to_string())),
        commit: head
            .and_then(|head| head.peel_to_commit().ok())
            .map(|commit| commit.id().to_string()),
    }
}

pub fn setup_manifest_repositories(manifest_path: &Path) -> Result<RepositorySetupOutput> {
    let manifest_path = absolute_path(manifest_path)?;
    anyhow::ensure!(
        manifest_path.is_file(),
        "Repository manifest not found: {}",
        manifest_path.display()
    );
    let manifest = load_manifest(&manifest_path)?;
    anyhow::ensure!(
        !manifest.repositories.is_empty(),
        "Repository manifest contains no repositories: {}",
        manifest_path.display()
    );
    let manifest_base = manifest_path.parent().unwrap_or(Path::new("."));
    let declarations = manifest
        .repositories
        .iter()
        .map(|declaration| resolve_declaration(declaration, manifest_base, true))
        .collect::<Result<Vec<_>>>()?;
    let mut checked = Vec::new();
    for entry in &declarations {
        merge_entry(&mut checked, entry.clone())?;
    }

    let device_path = device_config_path(&manifest_path)?;
    let git = GitConfig {
        ssh_key_path: config::load_device_ssh_key_path_from(&device_path)?,
        ..GitConfig::default()
    };
    let mut results = Vec::with_capacity(declarations.len());
    for entry in declarations {
        let result = match setup_one(&entry, &git) {
            Ok(status) => RepositorySetupResult {
                id: entry.id,
                name: entry.name,
                path: entry.path.display().to_string(),
                status,
                error: None,
            },
            Err(error) => RepositorySetupResult {
                id: entry.id,
                name: entry.name,
                path: entry.path.display().to_string(),
                status: RepositorySetupStatus::Error,
                error: Some(format!("{error:#}")),
            },
        };
        results.push(result);
    }
    let ok = results
        .iter()
        .all(|result| result.status != RepositorySetupStatus::Error);
    Ok(RepositorySetupOutput {
        ok,
        manifest_path: manifest_path.display().to_string(),
        repositories: results,
    })
}

fn setup_one(entry: &RepositoryEntry, git: &GitConfig) -> Result<RepositorySetupStatus> {
    if std::fs::symlink_metadata(&entry.path).is_ok() {
        validate_existing_repository(entry)?;
        return Ok(RepositorySetupStatus::Ready);
    }
    validate_checkout_path(&entry.path)?;
    let parent = entry
        .path
        .parent()
        .context("repository checkout path has no parent")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let temporary = unique_temporary_checkout_path(&entry.path);
    let clone_result: Result<()> = (|| {
        crate::git::clone_repository(&entry.remote, &entry.branch, &temporary, git)?;
        let config_relative = entry
            .config_path
            .strip_prefix(&entry.path)
            .with_context(|| {
                format!(
                    "config path {} must be inside a repository that needs cloning",
                    entry.config_path.display()
                )
            })?;
        let mut temporary_entry = entry.clone();
        temporary_entry.path = temporary.clone();
        temporary_entry.config_path = temporary.join(config_relative);
        validate_existing_repository(&temporary_entry)?;
        std::fs::rename(&temporary, &entry.path).with_context(|| {
            format!(
                "failed to move cloned repository into {}",
                entry.path.display()
            )
        })?;
        Ok(())
    })();
    if clone_result.is_err() && std::fs::symlink_metadata(&temporary).is_ok() {
        let _ = std::fs::remove_dir_all(&temporary);
    }
    clone_result?;
    Ok(RepositorySetupStatus::Cloned)
}

fn validate_existing_repository(entry: &RepositoryEntry) -> Result<()> {
    let repository = Repository::open(&entry.path)
        .with_context(|| format!("{} is not a Git repository", entry.path.display()))?;
    let workdir = repository
        .workdir()
        .context("bare Git repositories cannot be Keepbook data repositories")?;
    let expected = entry
        .path
        .canonicalize()
        .unwrap_or_else(|_| entry.path.clone());
    let actual = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());
    anyhow::ensure!(
        actual == expected,
        "{} is inside a different Git worktree rooted at {}",
        entry.path.display(),
        actual.display()
    );
    let actual_remote = repository
        .find_remote("origin")
        .context("Git repository has no origin remote")?
        .url()
        .context("origin remote has no URL")?
        .to_string();
    anyhow::ensure!(
        same_remote(&entry.remote, &actual_remote),
        "origin remote mismatch: expected {}, found {}",
        entry.remote,
        actual_remote
    );
    let branch = repository
        .head()
        .context("Git repository has no HEAD")?
        .shorthand()
        .context("Git repository HEAD is not a named UTF-8 branch")?
        .to_string();
    anyhow::ensure!(
        branch == entry.branch,
        "branch mismatch: expected {}, found {}",
        entry.branch,
        branch
    );
    anyhow::ensure!(
        entry.config_path.is_file(),
        "Keepbook config not found: {}",
        entry.config_path.display()
    );
    Config::load(&entry.config_path)
        .with_context(|| format!("invalid Keepbook config: {}", entry.config_path.display()))?;
    Ok(())
}

fn load_manifest_if_present(path: &Path) -> Result<RepositoryManifest> {
    if path.is_file() {
        load_manifest(path)
    } else {
        Ok(RepositoryManifest::default())
    }
}

fn load_manifest(path: &Path) -> Result<RepositoryManifest> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read repository manifest from {}", path.display()))?;
    toml::from_str(&content).with_context(|| {
        format!(
            "failed to parse repository manifest from {}",
            path.display()
        )
    })
}

fn load_device_file(path: &Path) -> Result<DeviceFile> {
    if !path.is_file() {
        return Ok(DeviceFile::default());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read device config from {}", path.display()))?;
    toml::from_str(&content)
        .with_context(|| format!("failed to parse device config from {}", path.display()))
}

fn save_device_repositories(path: &Path, state: &DeviceRepositories) -> Result<()> {
    let mut document = if path.is_file() {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read device config from {}", path.display()))?
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse device config from {}", path.display()))?
    } else {
        DocumentMut::new()
    };
    if document
        .get("repositories")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        document.insert("repositories", Item::Table(Table::new()));
    }
    if let Some(active) = &state.active_repository {
        document["repositories"]["active_repository"] = value(active);
    } else if let Some(repositories) = document["repositories"].as_table_like_mut() {
        repositories.remove("active_repository");
    }
    let mut local = ArrayOfTables::new();
    for declaration in &state.local {
        local.push(declaration_table(declaration));
    }
    document["repositories"]["local"] = Item::ArrayOfTables(local);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("device.toml");
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    std::fs::write(&temporary, document.to_string())
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("failed to replace {}", path.display()))
}

fn declaration_table(declaration: &RepositoryDeclaration) -> Table {
    let mut table = Table::new();
    table["id"] = value(&declaration.id);
    if !declaration.name.is_empty() {
        table["name"] = value(&declaration.name);
    }
    table["remote"] = value(&declaration.remote);
    table["branch"] = value(&declaration.branch);
    if let Some(path) = &declaration.path {
        table["path"] = value(path.display().to_string());
    }
    if let Some(path) = &declaration.config_path {
        table["config_path"] = value(path.display().to_string());
    }
    table
}

fn resolve_declaration(
    declaration: &RepositoryDeclaration,
    base: &Path,
    managed: bool,
) -> Result<RepositoryEntry> {
    let id = declaration.id.trim();
    anyhow::ensure!(!id.is_empty(), "Repository id is required");
    anyhow::ensure!(
        id.chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')),
        "Repository id {id:?} may contain only letters, numbers, '-' and '_'"
    );
    let remote = declaration.remote.trim();
    anyhow::ensure!(
        !remote.is_empty(),
        "Git remote is required for repository {id}"
    );
    let branch = declaration.branch.trim();
    anyhow::ensure!(
        !branch.is_empty(),
        "Git branch is required for repository {id}"
    );
    let path = match &declaration.path {
        Some(path) => resolve_path(base, path),
        None => default_repository_path(id)?,
    };
    validate_checkout_path(&path)?;
    let config_path = declaration
        .config_path
        .as_ref()
        .map(|config_path| resolve_path(&path, config_path))
        .unwrap_or_else(|| path.join("keepbook.toml"));
    Ok(RepositoryEntry {
        id: id.to_string(),
        name: if declaration.name.trim().is_empty() {
            id.to_string()
        } else {
            declaration.name.trim().to_string()
        },
        path,
        config_path,
        remote: remote.to_string(),
        branch: branch.to_string(),
        managed,
    })
}

fn declaration_from_entry(entry: &RepositoryEntry) -> RepositoryDeclaration {
    RepositoryDeclaration {
        id: entry.id.clone(),
        name: entry.name.clone(),
        remote: entry.remote.clone(),
        branch: entry.branch.clone(),
        path: Some(entry.path.clone()),
        config_path: Some(entry.config_path.clone()),
    }
}

fn merge_entry(entries: &mut Vec<RepositoryEntry>, mut candidate: RepositoryEntry) -> Result<()> {
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.id == candidate.id || entry.path == candidate.path)
    {
        let identical = existing.id == candidate.id
            && existing.path == candidate.path
            && existing.config_path == candidate.config_path
            && same_remote(&existing.remote, &candidate.remote)
            && existing.branch == candidate.branch;
        anyhow::ensure!(
            identical,
            "Conflicting repository declarations for id {} or path {}",
            candidate.id,
            candidate.path.display()
        );
        existing.managed |= candidate.managed;
        if candidate.managed {
            existing.name = std::mem::take(&mut candidate.name);
        }
        return Ok(());
    }
    entries.push(candidate);
    Ok(())
}

fn default_repository_path(id: &str) -> Result<PathBuf> {
    let data_dir = dirs::data_dir().context("cannot resolve the platform data directory")?;
    Ok(normalize_path(
        data_dir.join("keepbook").join("repositories").join(id),
    ))
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let path = expand_tilde(path);
    if path.is_absolute() {
        Ok(normalize_path(path))
    } else {
        Ok(normalize_path(
            std::env::current_dir()
                .context("cannot resolve the current directory")?
                .join(path),
        ))
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    let path = expand_tilde(path);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(base.join(path))
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let mut components = path.components();
    let Some(first) = components.next() else {
        return path.to_path_buf();
    };
    if first.as_os_str() != "~" {
        return path.to_path_buf();
    }
    let Some(home) = dirs::home_dir() else {
        return path.to_path_buf();
    };
    components.fold(home, |path, component| path.join(component.as_os_str()))
}

fn normalize_path(path: PathBuf) -> PathBuf {
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

fn validate_checkout_path(path: &Path) -> Result<()> {
    anyhow::ensure!(
        path.parent().is_some(),
        "Repository checkout cannot be a filesystem root: {}",
        path.display()
    );
    Ok(())
}

fn repository_id_slug(name: &str) -> String {
    let slug = name
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "repository".to_string()
    } else {
        slug.to_string()
    }
}

fn unique_temporary_checkout_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository");
    (0..)
        .map(|suffix| {
            parent.join(format!(
                ".{name}.keepbook-clone-{}-{suffix}",
                std::process::id()
            ))
        })
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .expect("temporary checkout suffixes are unbounded")
}

fn same_remote(left: &str, right: &str) -> bool {
    canonical_remote(left) == canonical_remote(right)
}

fn canonical_remote(remote: &str) -> String {
    let remote = remote.trim().trim_end_matches('/');
    if let Some(path) = remote.strip_prefix("file://") {
        return canonical_local_remote(Path::new(path));
    }
    if Path::new(remote).is_absolute() {
        return canonical_local_remote(Path::new(remote));
    }
    if let Some((scheme, rest)) = remote.split_once("://") {
        let _ = scheme;
        let rest = rest.rsplit_once('@').map(|(_, rest)| rest).unwrap_or(rest);
        if let Some((host, path)) = rest.split_once('/') {
            return format!(
                "{}:{}",
                host.to_ascii_lowercase(),
                path.trim_start_matches('/').trim_end_matches(".git")
            );
        }
    }
    if let Some((host, path)) = remote.split_once(':') {
        let host = host.rsplit_once('@').map(|(_, host)| host).unwrap_or(host);
        return format!(
            "{}:{}",
            host.to_ascii_lowercase(),
            path.trim_start_matches('/').trim_end_matches(".git")
        );
    }
    remote.trim_end_matches(".git").to_string()
}

fn canonical_local_remote(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| normalize_path(path.to_path_buf()))
        .display()
        .to_string()
}

#[cfg(test)]
#[path = "../tests/unit/repositories_tests.rs"]
mod repositories_tests;
