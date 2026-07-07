use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn configured_ssh_private_key_for_credentials_skips_missing_file() {
    let dir = TempDir::new().expect("temp dir should be created");
    let missing_key = dir.path().join(".ssh").join("missing_key");
    let git_config = GitConfig {
        ssh_key_path: Some(missing_key),
        ..GitConfig::default()
    };

    assert_eq!(
        configured_ssh_private_key_for_credentials(&git_config),
        None
    );
}

#[test]
fn configured_ssh_private_key_for_credentials_loads_existing_file() -> Result<()> {
    let dir = TempDir::new()?;
    let key_path = dir.path().join(".ssh").join("keepbook_sync_key");
    let key_contents = "test key\n";
    fs::create_dir_all(key_path.parent().expect("test key should have parent"))?;
    fs::write(&key_path, key_contents)?;
    let git_config = GitConfig {
        ssh_key_path: Some(key_path.clone()),
        ..GitConfig::default()
    };

    assert_eq!(
        configured_ssh_private_key_for_credentials(&git_config),
        Some(SshPrivateKey {
            path: key_path,
            private_key: key_contents.to_string()
        })
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn configured_ssh_private_key_for_credentials_skips_unreadable_file() -> Result<()> {
    let dir = TempDir::new()?;
    let key_path = dir.path().join(".ssh").join("keepbook_sync_key");
    fs::create_dir_all(key_path.parent().expect("test key should have parent"))?;
    fs::write(&key_path, "test key\n")?;
    let original_permissions = fs::metadata(&key_path)?.permissions();
    let mut unreadable_permissions = original_permissions.clone();
    unreadable_permissions.set_mode(0o000);
    fs::set_permissions(&key_path, unreadable_permissions)?;

    let git_config = GitConfig {
        ssh_key_path: Some(key_path.clone()),
        ..GitConfig::default()
    };

    let loaded_key = configured_ssh_private_key_for_credentials(&git_config);

    fs::set_permissions(&key_path, original_permissions)?;

    assert_eq!(loaded_key, None);
    Ok(())
}

#[test]
fn default_ssh_identity_paths_include_keepbook_sync_key() -> Result<()> {
    let dir = TempDir::new()?;
    let key_path = dir.path().join(".ssh").join("keepbook_sync_key");
    fs::create_dir_all(key_path.parent().expect("test key should have parent"))?;
    fs::write(&key_path, "test key")?;

    assert_eq!(
        default_ssh_identity_paths_in_home(dir.path()),
        vec![key_path]
    );
    Ok(())
}

#[test]
fn default_ssh_private_keys_for_credentials_loads_existing_files() -> Result<()> {
    let dir = TempDir::new()?;
    let key_path = dir.path().join(".ssh").join("keepbook_sync_key");
    let key_contents = "test key\n";
    fs::create_dir_all(key_path.parent().expect("test key should have parent"))?;
    fs::write(&key_path, key_contents)?;

    assert_eq!(
        default_ssh_private_keys_for_credentials_in_home(dir.path()),
        vec![SshPrivateKey {
            path: key_path,
            private_key: key_contents.to_string()
        }]
    );
    Ok(())
}

fn run_git(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    Ok(output)
}

fn init_repo(dir: &Path) -> Result<()> {
    let init = run_git(dir, &["init"])?;
    if !init.status.success() {
        anyhow::bail!("git init failed");
    }
    let email = run_git(dir, &["config", "user.email", "test@example.com"])?;
    if !email.status.success() {
        anyhow::bail!("git config user.email failed");
    }
    let name = run_git(dir, &["config", "user.name", "Keepbook Test"])?;
    if !name.status.success() {
        anyhow::bail!("git config user.name failed");
    }
    Ok(())
}

fn current_branch(dir: &Path) -> Result<String> {
    let out = run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !out.status.success() {
        anyhow::bail!("git rev-parse --abbrev-ref failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn commit_all(dir: &Path, message: &str) -> Result<()> {
    let add = run_git(dir, &["add", "-A"])?;
    if !add.status.success() {
        anyhow::bail!("git add failed");
    }
    let commit = run_git(dir, &["commit", "-m", message])?;
    if !commit.status.success() {
        anyhow::bail!("git commit failed");
    }
    Ok(())
}

fn push_tracking_branch(dir: &Path) -> Result<()> {
    let branch = current_branch(dir)?;
    let push = run_git(dir, &["push", "-u", "origin", &branch])?;
    if !push.status.success() {
        anyhow::bail!("git push -u failed");
    }
    Ok(())
}

fn merge_in_progress(dir: &Path) -> Result<bool> {
    let out = run_git(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"])?;
    Ok(out.status.success())
}

#[test]
fn test_auto_commit_skips_when_not_repo() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    let outcome = try_auto_commit(dir.path(), "test", &GitConfig::default())?;
    assert_eq!(
        outcome,
        AutoCommitOutcome::SkippedNotRepo {
            reason: "data directory is not a git repository".to_string()
        }
    );
    Ok(())
}

#[test]
fn test_auto_commit_skips_when_repo_root_mismatch() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;
    let data_dir = dir.path().join("data");
    fs::create_dir_all(&data_dir)?;

    let outcome = try_auto_commit(&data_dir, "test", &GitConfig::default())?;
    match outcome {
        AutoCommitOutcome::SkippedNotRepo { .. } => Ok(()),
        other => anyhow::bail!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn test_auto_commit_commits_changes() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    fs::write(dir.path().join("sample.txt"), "hello")?;

    let outcome = try_auto_commit(dir.path(), "sync mock", &GitConfig::default())?;
    assert_eq!(outcome, AutoCommitOutcome::Committed);

    let log = run_git(dir.path(), &["log", "-1", "--pretty=%s"])?;
    let subject = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(subject, "keepbook: sync mock");

    let status = run_git(dir.path(), &["status", "--porcelain"])?;
    let status_output = String::from_utf8_lossy(&status.stdout);
    assert!(status_output.trim().is_empty());

    Ok(())
}

#[test]
fn test_auto_commit_skips_when_no_changes() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    fs::write(dir.path().join("sample.txt"), "hello")?;
    let add = run_git(dir.path(), &["add", "-A"])?;
    if !add.status.success() {
        anyhow::bail!("git add failed");
    }
    let commit = run_git(dir.path(), &["commit", "-m", "initial"])?;
    if !commit.status.success() {
        anyhow::bail!("git commit failed");
    }

    let outcome = try_auto_commit(dir.path(), "sync mock", &GitConfig::default())?;
    assert_eq!(outcome, AutoCommitOutcome::SkippedNoChanges);

    Ok(())
}

#[test]
fn test_auto_commit_ignores_keepbook_config_changes() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    fs::write(dir.path().join("keepbook.toml"), "data_dir = \".\"\n")?;
    commit_all(dir.path(), "initial")?;

    fs::write(
        dir.path().join("keepbook.toml"),
        "data_dir = \"/Users/kat/Library/Application Support/keepbook\"\n",
    )?;

    let outcome = try_auto_commit(dir.path(), "sync mock", &GitConfig::default())?;
    assert_eq!(outcome, AutoCommitOutcome::SkippedNoChanges);

    let status = run_git(dir.path(), &["status", "--porcelain"])?;
    let status_output = String::from_utf8_lossy(&status.stdout);
    assert!(status_output.contains(" M keepbook.toml"));

    Ok(())
}

#[test]
fn test_auto_commit_commits_data_but_leaves_keepbook_config_unstaged() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    fs::write(dir.path().join("keepbook.toml"), "data_dir = \".\"\n")?;
    commit_all(dir.path(), "initial")?;

    fs::write(
        dir.path().join("keepbook.toml"),
        "data_dir = \"/Users/kat/Library/Application Support/keepbook\"\n",
    )?;
    fs::write(dir.path().join("balances.json"), "[]\n")?;

    let outcome = try_auto_commit(dir.path(), "sync mock", &GitConfig::default())?;
    assert_eq!(outcome, AutoCommitOutcome::Committed);

    let show = run_git(dir.path(), &["show", "--name-only", "--pretty=", "HEAD"])?;
    let committed_paths = String::from_utf8_lossy(&show.stdout);
    assert!(committed_paths.contains("balances.json"));
    assert!(!committed_paths.contains("keepbook.toml"));

    let status = run_git(dir.path(), &["status", "--porcelain"])?;
    let status_output = String::from_utf8_lossy(&status.stdout);
    assert!(status_output.contains(" M keepbook.toml"));

    Ok(())
}

#[test]
fn test_auto_commit_pushes_when_enabled() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let remote = TempDir::new()?;
    let remote_init = run_git(remote.path(), &["init", "--bare"])?;
    if !remote_init.status.success() {
        anyhow::bail!("git init --bare failed");
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    let remote_path = remote.path().to_string_lossy().to_string();
    let add_remote = run_git(dir.path(), &["remote", "add", "origin", &remote_path])?;
    if !add_remote.status.success() {
        anyhow::bail!("git remote add failed");
    }

    fs::write(dir.path().join("initial.txt"), "initial")?;
    let add = run_git(dir.path(), &["add", "-A"])?;
    if !add.status.success() {
        anyhow::bail!("git add failed");
    }
    let commit = run_git(dir.path(), &["commit", "-m", "initial"])?;
    if !commit.status.success() {
        anyhow::bail!("git commit failed");
    }

    let branch_output = run_git(dir.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !branch_output.status.success() {
        anyhow::bail!("git rev-parse failed");
    }
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    let push_initial = run_git(dir.path(), &["push", "-u", "origin", &branch])?;
    if !push_initial.status.success() {
        anyhow::bail!("git push -u failed");
    }

    fs::write(dir.path().join("sample.txt"), "hello")?;

    let outcome = try_auto_commit(
        dir.path(),
        "sync mock",
        &GitConfig {
            auto_push: true,
            ..GitConfig::default()
        },
    )?;
    assert_eq!(outcome, AutoCommitOutcome::Committed);

    let remote_log = run_git(remote.path(), &["log", "-1", "--pretty=%s"])?;
    if !remote_log.status.success() {
        anyhow::bail!("git log failed on remote");
    }
    let remote_subject = String::from_utf8_lossy(&remote_log.stdout)
        .trim()
        .to_string();
    assert_eq!(remote_subject, "keepbook: sync mock");

    Ok(())
}

#[test]
fn test_merge_origin_master_skips_when_not_repo() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    let outcome = try_merge_origin_master(dir.path(), &GitConfig::default())?;
    assert_eq!(
        outcome,
        MergeOriginMasterOutcome::SkippedNotRepo {
            reason: "data directory is not a git repository".to_string()
        }
    );
    Ok(())
}

#[test]
fn test_merge_origin_master_merges_remote_master() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let remote = TempDir::new()?;
    let remote_init = run_git(remote.path(), &["init", "--bare"])?;
    if !remote_init.status.success() {
        anyhow::bail!("git init --bare failed");
    }

    let source = TempDir::new()?;
    init_repo(source.path())?;
    let remote_path = remote.path().to_string_lossy().to_string();
    let add_remote = run_git(source.path(), &["remote", "add", "origin", &remote_path])?;
    if !add_remote.status.success() {
        anyhow::bail!("git remote add failed");
    }
    fs::write(source.path().join("sample.txt"), "base\n")?;
    commit_all(source.path(), "base")?;
    push_tracking_branch(source.path())?;

    let local = TempDir::new()?;
    let clone = run_git(local.path(), &["clone", &remote_path, "."])?;
    if !clone.status.success() {
        anyhow::bail!("git clone failed");
    }
    let email = run_git(local.path(), &["config", "user.email", "test@example.com"])?;
    if !email.status.success() {
        anyhow::bail!("git config user.email failed");
    }
    let name = run_git(local.path(), &["config", "user.name", "Keepbook Test"])?;
    if !name.status.success() {
        anyhow::bail!("git config user.name failed");
    }

    fs::write(source.path().join("sample.txt"), "base\nremote\n")?;
    commit_all(source.path(), "remote update")?;
    let push = run_git(source.path(), &["push"])?;
    if !push.status.success() {
        anyhow::bail!("git push failed");
    }

    let outcome = try_merge_origin_master(local.path(), &GitConfig::default())?;
    assert_eq!(outcome, MergeOriginMasterOutcome::Merged);

    let local_content = fs::read_to_string(local.path().join("sample.txt"))?;
    assert!(local_content.contains("remote"));

    Ok(())
}

#[test]
fn test_merge_origin_master_aborts_on_conflicts() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let remote = TempDir::new()?;
    let remote_init = run_git(remote.path(), &["init", "--bare"])?;
    if !remote_init.status.success() {
        anyhow::bail!("git init --bare failed");
    }

    let source = TempDir::new()?;
    init_repo(source.path())?;
    let remote_path = remote.path().to_string_lossy().to_string();
    let add_remote = run_git(source.path(), &["remote", "add", "origin", &remote_path])?;
    if !add_remote.status.success() {
        anyhow::bail!("git remote add failed");
    }
    fs::write(source.path().join("conflict.txt"), "line\n")?;
    commit_all(source.path(), "base")?;
    push_tracking_branch(source.path())?;

    let local = TempDir::new()?;
    let clone = run_git(local.path(), &["clone", &remote_path, "."])?;
    if !clone.status.success() {
        anyhow::bail!("git clone failed");
    }
    let email = run_git(local.path(), &["config", "user.email", "test@example.com"])?;
    if !email.status.success() {
        anyhow::bail!("git config user.email failed");
    }
    let name = run_git(local.path(), &["config", "user.name", "Keepbook Test"])?;
    if !name.status.success() {
        anyhow::bail!("git config user.name failed");
    }

    let checkout = run_git(local.path(), &["checkout", "-b", "work"])?;
    if !checkout.status.success() {
        anyhow::bail!("git checkout -b work failed");
    }

    fs::write(local.path().join("conflict.txt"), "local\n")?;
    commit_all(local.path(), "local change")?;

    fs::write(source.path().join("conflict.txt"), "remote\n")?;
    commit_all(source.path(), "remote change")?;
    let push = run_git(source.path(), &["push"])?;
    if !push.status.success() {
        anyhow::bail!("git push failed");
    }

    let outcome = try_merge_origin_master(local.path(), &GitConfig::default())?;
    assert_eq!(outcome, MergeOriginMasterOutcome::ConflictAborted);
    assert!(!merge_in_progress(local.path())?);

    let content = fs::read_to_string(local.path().join("conflict.txt"))?;
    assert_eq!(content, "local\n");

    Ok(())
}
