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

pub fn try_auto_commit(data_dir: &Path, action: &str) -> Result<AutoCommitOutcome> {
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

    Ok(AutoCommitOutcome::Committed)
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
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()?;
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

    #[test]
    fn test_auto_commit_skips_when_not_repo() -> Result<()> {
        if !git_available() {
            return Ok(());
        }

        let dir = TempDir::new()?;
        let outcome = try_auto_commit(dir.path(), "test")?;
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

        let outcome = try_auto_commit(&data_dir, "test")?;
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

        let outcome = try_auto_commit(dir.path(), "sync mock")?;
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

        let outcome = try_auto_commit(dir.path(), "sync mock")?;
        assert_eq!(outcome, AutoCommitOutcome::SkippedNoChanges);

        Ok(())
    }
}
