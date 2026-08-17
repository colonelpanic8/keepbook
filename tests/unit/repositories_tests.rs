use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{Repository, RepositoryInitOptions, Signature};
use tempfile::TempDir;

use super::*;

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

fn create_remote(root: &Path, name: &str, with_config: bool) -> Result<PathBuf> {
    let path = root.join(name);
    let mut options = RepositoryInitOptions::new();
    options.initial_head("main");
    let repository = Repository::init_opts(&path, &options)?;
    if with_config {
        write_file(
            &path.join("keepbook.toml"),
            "reporting_currency = \"USD\"\n",
        )?;
    } else {
        write_file(&path.join("README.md"), "missing keepbook config\n")?;
    }
    let mut index = repository.index()?;
    index.add_all(["."], git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repository.find_tree(tree_id)?;
    let signature = Signature::now("Keepbook Tests", "keepbook@example.invalid")?;
    repository.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])?;
    Ok(path)
}

fn manifest_entry(id: &str, remote: &Path, path: &Path) -> String {
    format!(
        "[[repositories]]\nid = \"{id}\"\nremote = \"{}\"\nbranch = \"main\"\npath = \"{}\"\n",
        remote.display(),
        path.display()
    )
}

#[test]
fn manifest_resolves_legacy_default_relative_paths_and_device_precedence() -> Result<()> {
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("config/app.toml");
    let device_path = temp.path().join("config/device.toml");
    write_file(
        &manifest_path,
        r#"
active_repository = "managed"

[[repositories]]
id = "managed"
remote = "git@github.com:owner/managed.git"
path = "../data/managed"

[[repositories]]
id = "default-path"
name = "Default Path"
remote = "https://github.com/owner/default-path.git"
"#,
    )?;
    write_file(
        &device_path,
        r#"
[git]
ssh_key_path = "./id_ed25519"

[repositories]
active_repository = "local"

[[repositories.local]]
id = "local"
remote = "git@github.com:owner/local.git"
path = "../data/local"
"#,
    )?;

    let registry = load_registry_from_paths(&manifest_path, &device_path)?;
    assert_eq!(registry.default_repository.as_deref(), Some("managed"));
    assert_eq!(registry.active_repository.as_deref(), Some("local"));
    assert_eq!(registry.repositories.len(), 3);
    assert_eq!(
        registry.repositories[0].path,
        temp.path().join("data/managed")
    );
    assert_eq!(registry.repositories[0].name, "managed");
    assert!(registry.repositories[0].managed);
    assert!(!registry.repositories[2].managed);
    assert!(registry.repositories[1]
        .path
        .ends_with("keepbook/repositories/default-path"));
    Ok(())
}

#[test]
fn identical_device_entry_collapses_into_managed_entry_and_conflicts_fail() -> Result<()> {
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("app.toml");
    let device_path = temp.path().join("device.toml");
    let checkout = temp.path().join("data");
    write_file(
        &manifest_path,
        &format!(
            "[[repositories]]\nid = \"books\"\nname = \"Books\"\nremote = \"git@github.com:owner/books.git\"\npath = \"{}\"\n",
            checkout.display()
        ),
    )?;
    write_file(
        &device_path,
        &format!(
            "[[repositories.local]]\nid = \"books\"\nremote = \"https://github.com/owner/books.git\"\npath = \"{}\"\n",
            checkout.display()
        ),
    )?;

    let registry = load_registry_from_paths(&manifest_path, &device_path)?;
    assert_eq!(registry.repositories.len(), 1);
    assert!(registry.repositories[0].managed);

    write_file(
        &device_path,
        &format!(
            "[[repositories.local]]\nid = \"books\"\nremote = \"git@github.com:other/books.git\"\npath = \"{}\"\n",
            checkout.display()
        ),
    )?;
    let error = load_registry_from_paths(&manifest_path, &device_path).unwrap_err();
    assert!(error
        .to_string()
        .contains("Conflicting repository declarations"));
    Ok(())
}

#[test]
fn device_state_updates_preserve_other_settings_and_manifest_contents() -> Result<()> {
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("app.toml");
    let device_path = temp.path().join("device.toml");
    let manifest = "[[repositories]]\nid = \"managed\"\nremote = \"git@example.com:managed.git\"\n";
    write_file(&manifest_path, manifest)?;
    write_file(&device_path, "[git]\nssh_key_path = \"./key\"\n")?;
    let state = DeviceRepositories {
        active_repository: Some("local".to_string()),
        local: vec![RepositoryDeclaration {
            id: "local".to_string(),
            name: "Local".to_string(),
            remote: "git@example.com:local.git".to_string(),
            branch: "main".to_string(),
            path: Some(temp.path().join("local")),
            config_path: None,
        }],
    };
    save_device_repositories(&device_path, &state)?;

    assert_eq!(std::fs::read_to_string(&manifest_path)?, manifest);
    let device = std::fs::read_to_string(&device_path)?;
    assert!(device.contains("ssh_key_path = \"./key\""));
    assert!(device.contains("active_repository = \"local\""));
    assert!(device.contains("[[repositories.local]]"));
    Ok(())
}

#[test]
fn managed_repository_cannot_be_removed_from_device_state() -> Result<()> {
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("app.toml");
    let device_path = temp.path().join("device.toml");
    write_file(
        &manifest_path,
        "[[repositories]]\nid = \"managed\"\nremote = \"git@example.com:managed.git\"\n",
    )?;
    let error =
        remove_local_repository_from_paths(&manifest_path, &device_path, "managed").unwrap_err();
    assert!(error.to_string().contains("managed by"));
    assert!(!device_path.exists());
    Ok(())
}

#[test]
fn local_repository_add_activate_and_remove_use_only_device_state() -> Result<()> {
    let temp = TempDir::new()?;
    let manifest_path = temp.path().join("app.toml");
    let device_path = temp.path().join("device.toml");
    let manifest = "[[repositories]]\nid = \"managed\"\nremote = \"git@example.com:managed.git\"\n";
    write_file(&manifest_path, manifest)?;
    let local = RepositoryDeclaration {
        id: "local".to_string(),
        name: "Local".to_string(),
        remote: "git@example.com:local.git".to_string(),
        branch: "main".to_string(),
        path: Some(temp.path().join("local")),
        config_path: None,
    };

    let added = add_local_repository_from_paths(&manifest_path, &device_path, local)?;
    assert_eq!(added.repositories.len(), 2);
    assert!(!added.repositories[1].managed);
    let active = set_active_repository_from_paths(&manifest_path, &device_path, "local")?;
    assert_eq!(active.active_repository.as_deref(), Some("local"));
    let error =
        remove_local_repository_from_paths(&manifest_path, &device_path, "local").unwrap_err();
    assert!(error.to_string().contains("Switch to another repository"));

    set_active_repository_from_paths(&manifest_path, &device_path, "managed")?;
    let removed = remove_local_repository_from_paths(&manifest_path, &device_path, "local")?;
    assert_eq!(removed.repositories.len(), 1);
    assert_eq!(std::fs::read_to_string(&manifest_path)?, manifest);
    Ok(())
}

#[test]
fn setup_clones_missing_repository_and_rerun_is_ready() -> Result<()> {
    let temp = TempDir::new()?;
    let remote = create_remote(temp.path(), "remote", true)?;
    let checkout = temp.path().join("checkouts/personal");
    let manifest_path = temp.path().join("app.toml");
    write_file(
        &manifest_path,
        &manifest_entry("personal", &remote, &checkout),
    )?;

    let first = setup_manifest_repositories(&manifest_path)?;
    assert!(first.ok);
    assert_eq!(first.repositories[0].status, RepositorySetupStatus::Cloned);
    assert!(checkout.join("keepbook.toml").is_file());

    let second = setup_manifest_repositories(&manifest_path)?;
    assert!(second.ok);
    assert_eq!(second.repositories[0].status, RepositorySetupStatus::Ready);
    Ok(())
}

#[test]
fn setup_continues_after_conflict_and_reports_json_shape() -> Result<()> {
    let temp = TempDir::new()?;
    let remote = create_remote(temp.path(), "remote", true)?;
    let good = temp.path().join("checkouts/good");
    let occupied = temp.path().join("checkouts/occupied");
    std::fs::create_dir_all(&occupied)?;
    let manifest_path = temp.path().join("app.toml");
    write_file(
        &manifest_path,
        &format!(
            "{}\n{}",
            manifest_entry("good", &remote, &good),
            manifest_entry("occupied", &remote, &occupied)
        ),
    )?;

    let output = setup_manifest_repositories(&manifest_path)?;
    assert!(!output.ok);
    assert_eq!(output.repositories[0].status, RepositorySetupStatus::Cloned);
    assert_eq!(output.repositories[1].status, RepositorySetupStatus::Error);
    let json = serde_json::to_value(&output)?;
    assert!(json["repositories"][0].get("error").is_none());
    assert!(json["repositories"][1]["error"].is_string());
    assert!(good.is_dir());
    Ok(())
}

#[test]
fn failed_clone_validation_leaves_no_target_or_temporary_checkout() -> Result<()> {
    let temp = TempDir::new()?;
    let remote = create_remote(temp.path(), "remote", false)?;
    let checkout = temp.path().join("checkouts/personal");
    let manifest_path = temp.path().join("app.toml");
    write_file(
        &manifest_path,
        &manifest_entry("personal", &remote, &checkout),
    )?;

    let output = setup_manifest_repositories(&manifest_path)?;
    assert!(!output.ok);
    assert!(!checkout.exists());
    let leftovers = std::fs::read_dir(checkout.parent().unwrap())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("keepbook-clone")
        })
        .count();
    assert_eq!(leftovers, 0);
    Ok(())
}

#[test]
fn existing_repository_remote_and_branch_mismatches_are_errors() -> Result<()> {
    let temp = TempDir::new()?;
    let remote = create_remote(temp.path(), "remote", true)?;
    let checkout = temp.path().join("checkout");
    let manifest_path = temp.path().join("app.toml");
    write_file(
        &manifest_path,
        &manifest_entry("personal", &remote, &checkout),
    )?;
    assert!(setup_manifest_repositories(&manifest_path)?.ok);

    write_file(
        &manifest_path,
        &manifest_entry("personal", Path::new("/different/remote"), &checkout),
    )?;
    let remote_output = setup_manifest_repositories(&manifest_path)?;
    assert!(!remote_output.ok);
    assert!(remote_output.repositories[0]
        .error
        .as_deref()
        .unwrap()
        .contains("origin remote mismatch"));

    write_file(
        &manifest_path,
        &manifest_entry("personal", &remote, &checkout)
            .replace("branch = \"main\"", "branch = \"other\""),
    )?;
    let branch_output = setup_manifest_repositories(&manifest_path)?;
    assert!(!branch_output.ok);
    assert!(branch_output.repositories[0]
        .error
        .as_deref()
        .unwrap()
        .contains("branch mismatch"));
    Ok(())
}
