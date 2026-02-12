use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, PartialEq, Eq)]
pub enum AutoCommitOutcome {
    SkippedNotRepo { reason: String },
    SkippedNoChanges,
    Committed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MergeOriginMasterOutcome {
    SkippedNotRepo { reason: String },
    UpToDate,
    Merged,
    ConflictAborted,
}

pub fn try_auto_commit(
    data_dir: &Path,
    action: &str,
    auto_push: bool,
) -> Result<AutoCommitOutcome> {
    let repo_root = git_repo_root(data_dir)?;
    let Some(repo_root) = repo_root else {
        return Ok(AutoCommitOutcome::SkippedNotRepo {
            reason: "data directory is not a git repository".to_string(),
        });
    };

    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.clone());
    let data_dir = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());

    if repo_root != data_dir {
        return Ok(AutoCommitOutcome::SkippedNotRepo {
            reason: format!(
                "data directory is not the git repo root (repo root: {})",
                repo_root.display()
            ),
        });
    }

    let status = git_output(&data_dir, &["status", "--porcelain"])?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("git status failed: {stderr}");
    }
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    if status_stdout.trim().is_empty() {
        return Ok(AutoCommitOutcome::SkippedNoChanges);
    }

    let add = git_output(&data_dir, &["add", "-A"])?;
    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        anyhow::bail!("git add failed: {stderr}");
    }

    let action = action.trim();
    let message = if action.is_empty() {
        "keepbook: update data".to_string()
    } else {
        format!("keepbook: {action}")
    };

    let commit = git_output(&data_dir, &["commit", "-m", &message])?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        anyhow::bail!("git commit failed: {stderr}");
    }

    if auto_push {
        let push = git_output(&data_dir, &["push"])?;
        if !push.status.success() {
            let stderr = String::from_utf8_lossy(&push.stderr);
            anyhow::bail!("git push failed: {stderr}");
        }
    }

    Ok(AutoCommitOutcome::Committed)
}

pub fn try_merge_origin_master(data_dir: &Path) -> Result<MergeOriginMasterOutcome> {
    let repo_root = git_repo_root(data_dir)?;
    let Some(repo_root) = repo_root else {
        return Ok(MergeOriginMasterOutcome::SkippedNotRepo {
            reason: "data directory is not a git repository".to_string(),
        });
    };

    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.clone());
    let data_dir = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());

    if repo_root != data_dir {
        return Ok(MergeOriginMasterOutcome::SkippedNotRepo {
            reason: format!(
                "data directory is not the git repo root (repo root: {})",
                repo_root.display()
            ),
        });
    }

    let status = git_output(&data_dir, &["status", "--porcelain"])?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("git status failed: {stderr}");
    }
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    if !status_stdout.trim().is_empty() {
        anyhow::bail!("git working tree is not clean; cannot merge origin/master");
    }

    let head_before = git_output(&data_dir, &["rev-parse", "HEAD"])?;
    if !head_before.status.success() {
        let stderr = String::from_utf8_lossy(&head_before.stderr);
        anyhow::bail!("git rev-parse HEAD failed: {stderr}");
    }
    let head_before = String::from_utf8_lossy(&head_before.stdout).trim().to_string();

    let fetch = git_output(&data_dir, &["fetch", "origin", "master"])?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        anyhow::bail!("git fetch origin master failed: {stderr}");
    }

    let merge = git_output(&data_dir, &["merge", "--no-edit", "origin/master"])?;
    if merge.status.success() {
        let head_after = git_output(&data_dir, &["rev-parse", "HEAD"])?;
        if !head_after.status.success() {
            let stderr = String::from_utf8_lossy(&head_after.stderr);
            anyhow::bail!("git rev-parse HEAD failed: {stderr}");
        }
        let head_after = String::from_utf8_lossy(&head_after.stdout).trim().to_string();
        if head_before == head_after {
            return Ok(MergeOriginMasterOutcome::UpToDate);
        }
        return Ok(MergeOriginMasterOutcome::Merged);
    }

    if has_unmerged_files(&data_dir)? {
        let abort = git_output(&data_dir, &["merge", "--abort"])?;
        if !abort.status.success() {
            let stderr = String::from_utf8_lossy(&abort.stderr);
            anyhow::bail!(
                "git merge origin/master had conflicts and git merge --abort failed: {stderr}"
            );
        }
        return Ok(MergeOriginMasterOutcome::ConflictAborted);
    }

    let stderr = String::from_utf8_lossy(&merge.stderr);
    anyhow::bail!("git merge origin/master failed: {stderr}")
}

fn git_repo_root(dir: &Path) -> Result<Option<PathBuf>> {
    let output = git_output(dir, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        return Ok(None);
    }

    let root = String::from_utf8(output.stdout).context("Git repo root is not valid UTF-8")?;
    let root = root.trim();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(root)))
}

fn git_output(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                anyhow::anyhow!("git not found in PATH")
            } else {
                e.into()
            }
        })?;
    Ok(output)
}

fn has_unmerged_files(dir: &Path) -> Result<bool> {
    let output = git_output(dir, &["diff", "--name-only", "--diff-filter=U"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff --name-only --diff-filter=U failed: {stderr}");
    }
    let names = String::from_utf8_lossy(&output.stdout);
    Ok(!names.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
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
        let outcome = try_auto_commit(dir.path(), "test", false)?;
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

        let outcome = try_auto_commit(&data_dir, "test", false)?;
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

        let outcome = try_auto_commit(dir.path(), "sync mock", false)?;
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

        let outcome = try_auto_commit(dir.path(), "sync mock", false)?;
        assert_eq!(outcome, AutoCommitOutcome::SkippedNoChanges);

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

        let outcome = try_auto_commit(dir.path(), "sync mock", true)?;
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
        let outcome = try_merge_origin_master(dir.path())?;
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

        let outcome = try_merge_origin_master(local.path())?;
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

        let outcome = try_merge_origin_master(local.path())?;
        assert_eq!(outcome, MergeOriginMasterOutcome::ConflictAborted);
        assert!(!merge_in_progress(local.path())?);

        let content = fs::read_to_string(local.path().join("conflict.txt"))?;
        assert_eq!(content, "local\n");

        Ok(())
    }
}
