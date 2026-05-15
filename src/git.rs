use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

const AUTO_COMMIT_PATHSPEC: &[&str] = &["--", ".", ":!keepbook.toml"];

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

#[derive(Debug, PartialEq, Eq)]
pub enum PullRemoteOutcome {
    SkippedNotRepo { reason: String },
    SkippedNoUpstream { reason: String },
    UpToDate,
    Pulled,
    ConflictAborted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PushRemoteOutcome {
    SkippedNotRepo { reason: String },
    Pushed,
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

    let mut status_args = vec!["status", "--porcelain"];
    status_args.extend_from_slice(AUTO_COMMIT_PATHSPEC);
    let status = git_output(&data_dir, &status_args)?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("git status failed: {stderr}");
    }
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    if status_stdout.trim().is_empty() {
        return Ok(AutoCommitOutcome::SkippedNoChanges);
    }

    let mut add_args = vec!["add", "-A"];
    add_args.extend_from_slice(AUTO_COMMIT_PATHSPEC);
    let add = git_output(&data_dir, &add_args)?;
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
    let head_before = String::from_utf8_lossy(&head_before.stdout)
        .trim()
        .to_string();

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
        let head_after = String::from_utf8_lossy(&head_after.stdout)
            .trim()
            .to_string();
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

pub fn try_pull_remote(data_dir: &Path) -> Result<PullRemoteOutcome> {
    let repo_root = git_repo_root(data_dir)?;
    let Some(repo_root) = repo_root else {
        return Ok(PullRemoteOutcome::SkippedNotRepo {
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
        return Ok(PullRemoteOutcome::SkippedNotRepo {
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
        anyhow::bail!("git working tree is not clean; cannot pull remote changes");
    }

    let upstream = git_output(
        &data_dir,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )?;
    if !upstream.status.success() {
        return Ok(PullRemoteOutcome::SkippedNoUpstream {
            reason: "current branch does not have an upstream tracking branch".to_string(),
        });
    }
    let upstream_ref = String::from_utf8_lossy(&upstream.stdout).trim().to_string();
    if upstream_ref.is_empty() {
        return Ok(PullRemoteOutcome::SkippedNoUpstream {
            reason: "current branch does not have an upstream tracking branch".to_string(),
        });
    }

    let head_before = git_output(&data_dir, &["rev-parse", "HEAD"])?;
    if !head_before.status.success() {
        let stderr = String::from_utf8_lossy(&head_before.stderr);
        anyhow::bail!("git rev-parse HEAD failed: {stderr}");
    }
    let head_before = String::from_utf8_lossy(&head_before.stdout)
        .trim()
        .to_string();

    let fetch = git_output(&data_dir, &["fetch"])?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        anyhow::bail!("git fetch failed: {stderr}");
    }

    let merge = git_output(&data_dir, &["merge", "--no-edit", upstream_ref.as_str()])?;
    if merge.status.success() {
        let head_after = git_output(&data_dir, &["rev-parse", "HEAD"])?;
        if !head_after.status.success() {
            let stderr = String::from_utf8_lossy(&head_after.stderr);
            anyhow::bail!("git rev-parse HEAD failed: {stderr}");
        }
        let head_after = String::from_utf8_lossy(&head_after.stdout)
            .trim()
            .to_string();
        if head_before == head_after {
            return Ok(PullRemoteOutcome::UpToDate);
        }
        return Ok(PullRemoteOutcome::Pulled);
    }

    if has_unmerged_files(&data_dir)? {
        let abort = git_output(&data_dir, &["merge", "--abort"])?;
        if !abort.status.success() {
            let stderr = String::from_utf8_lossy(&abort.stderr);
            anyhow::bail!("git pull had conflicts and git merge --abort failed: {stderr}");
        }
        return Ok(PullRemoteOutcome::ConflictAborted);
    }

    let stderr = String::from_utf8_lossy(&merge.stderr);
    anyhow::bail!("git merge {upstream_ref} failed: {stderr}")
}

pub fn try_push_remote(data_dir: &Path) -> Result<PushRemoteOutcome> {
    let repo_root = git_repo_root(data_dir)?;
    let Some(repo_root) = repo_root else {
        return Ok(PushRemoteOutcome::SkippedNotRepo {
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
        return Ok(PushRemoteOutcome::SkippedNotRepo {
            reason: format!(
                "data directory is not the git repo root (repo root: {})",
                repo_root.display()
            ),
        });
    }

    let push = git_output(&data_dir, &["push"])?;
    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr);
        anyhow::bail!("git push failed: {stderr}");
    }

    Ok(PushRemoteOutcome::Pushed)
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
#[path = "../tests/unit/git_tests.rs"]
mod git_tests;
