use std::path::Path;

use crate::config::GitConfig;
use anyhow::{Context, Result};
use git2::build::CheckoutBuilder;
use git2::{
    BranchType, Cred, CredentialType, ErrorCode, FetchOptions, IndexAddOption, MergeOptions,
    PushOptions, RemoteCallbacks, Repository, Signature, StatusOptions,
};

const AUTO_COMMIT_EXCLUDED_PATH: &str = "keepbook.toml";
const DEFAULT_SSH_IDENTITY_FILES: &[&str] = &[
    "id_ed25519",
    "id_rsa",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_ed25519_sk",
    "keepbook_sync_key",
];

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
    git_config: &GitConfig,
) -> Result<AutoCommitOutcome> {
    let repo = match open_data_repo(data_dir)? {
        DataRepo::Found(repo) => repo,
        DataRepo::Skipped { reason } => return Ok(AutoCommitOutcome::SkippedNotRepo { reason }),
    };

    if !data_changes_present(&repo)? {
        return Ok(AutoCommitOutcome::SkippedNoChanges);
    }

    stage_data_changes(&repo)?;

    let mut index = repo.index().context("failed to open git index")?;
    let tree_id = index.write_tree().context("failed to write git tree")?;

    let parent = head_commit(&repo)?;
    if let Some(parent) = &parent {
        if parent.tree_id() == tree_id {
            return Ok(AutoCommitOutcome::SkippedNoChanges);
        }
    }

    let tree = repo.find_tree(tree_id).context("failed to read git tree")?;
    let signature = signature(&repo)?;
    let action = action.trim();
    let message = if action.is_empty() {
        "keepbook: update data".to_string()
    } else {
        format!("keepbook: {action}")
    };
    let parents: Vec<_> = parent.iter().collect();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &parents,
    )
    .context("git commit failed")?;

    if git_config.auto_push {
        push_current_branch(&repo, git_config).context("git push failed")?;
    }

    Ok(AutoCommitOutcome::Committed)
}

pub fn try_merge_origin_master(
    data_dir: &Path,
    git_config: &GitConfig,
) -> Result<MergeOriginMasterOutcome> {
    let repo = match open_data_repo(data_dir)? {
        DataRepo::Found(repo) => repo,
        DataRepo::Skipped { reason } => {
            return Ok(MergeOriginMasterOutcome::SkippedNotRepo { reason });
        }
    };

    ensure_clean_worktree(&repo, "cannot merge origin/master")?;
    fetch_remote(&repo, "origin", &["master"], git_config)
        .context("git fetch origin master failed")?;

    match merge_ref(&repo, "refs/remotes/origin/master", "origin/master")? {
        MergeResult::UpToDate => Ok(MergeOriginMasterOutcome::UpToDate),
        MergeResult::Merged => Ok(MergeOriginMasterOutcome::Merged),
        MergeResult::Conflict => Ok(MergeOriginMasterOutcome::ConflictAborted),
    }
}

pub fn try_pull_remote(data_dir: &Path, git_config: &GitConfig) -> Result<PullRemoteOutcome> {
    let repo = match open_data_repo(data_dir)? {
        DataRepo::Found(repo) => repo,
        DataRepo::Skipped { reason } => return Ok(PullRemoteOutcome::SkippedNotRepo { reason }),
    };

    ensure_clean_worktree(&repo, "cannot pull remote changes")?;

    let Some(upstream) = upstream_tracking_ref(&repo)? else {
        return Ok(PullRemoteOutcome::SkippedNoUpstream {
            reason: "current branch does not have an upstream tracking branch".to_string(),
        });
    };
    let Some((remote, branch)) = parse_remote_tracking_ref(&upstream) else {
        return Ok(PullRemoteOutcome::SkippedNoUpstream {
            reason: format!("unsupported upstream tracking branch: {upstream}"),
        });
    };

    fetch_remote(&repo, &remote, &[&branch], git_config).context("git fetch failed")?;

    match merge_ref(&repo, &upstream, &upstream)? {
        MergeResult::UpToDate => Ok(PullRemoteOutcome::UpToDate),
        MergeResult::Merged => Ok(PullRemoteOutcome::Pulled),
        MergeResult::Conflict => Ok(PullRemoteOutcome::ConflictAborted),
    }
}

pub fn try_push_remote(data_dir: &Path, git_config: &GitConfig) -> Result<PushRemoteOutcome> {
    let repo = match open_data_repo(data_dir)? {
        DataRepo::Found(repo) => repo,
        DataRepo::Skipped { reason } => return Ok(PushRemoteOutcome::SkippedNotRepo { reason }),
    };

    push_current_branch(&repo, git_config).context("git push failed")?;
    Ok(PushRemoteOutcome::Pushed)
}

enum DataRepo {
    Found(Repository),
    Skipped { reason: String },
}

fn open_data_repo(data_dir: &Path) -> Result<DataRepo> {
    let repo = match Repository::discover(data_dir) {
        Ok(repo) => repo,
        Err(err) if err.code() == ErrorCode::NotFound => {
            return Ok(DataRepo::Skipped {
                reason: "data directory is not a git repository".to_string(),
            });
        }
        Err(err) => return Err(err).context("failed to open git repository"),
    };

    let repo_root = repo
        .workdir()
        .context("data directory git repository is bare")?
        .canonicalize()
        .unwrap_or_else(|_| repo.workdir().unwrap().to_path_buf());
    let data_dir = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());

    if repo_root != data_dir {
        return Ok(DataRepo::Skipped {
            reason: format!(
                "data directory is not the git repo root (repo root: {})",
                repo_root.display()
            ),
        });
    }

    Ok(DataRepo::Found(repo))
}

fn data_changes_present(repo: &Repository) -> Result<bool> {
    for entry in statuses(repo, true)?.iter() {
        let path = entry.path().context("git status path is not valid UTF-8")?;
        if path != AUTO_COMMIT_EXCLUDED_PATH {
            return Ok(true);
        }
    }
    Ok(false)
}

fn stage_data_changes(repo: &Repository) -> Result<()> {
    let mut index = repo.index().context("failed to open git index")?;
    index
        .add_all(
            ["."],
            IndexAddOption::DEFAULT,
            Some(&mut |path, _matched| i32::from(path == Path::new(AUTO_COMMIT_EXCLUDED_PATH))),
        )
        .context("git add failed")?;
    index.write().context("failed to write git index")?;
    Ok(())
}

fn ensure_clean_worktree(repo: &Repository, action: &str) -> Result<()> {
    if statuses(repo, true)?.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("git working tree is not clean; {action}");
    }
}

fn statuses(repo: &Repository, include_untracked: bool) -> Result<git2::Statuses<'_>> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(include_untracked)
        .recurse_untracked_dirs(include_untracked)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    repo.statuses(Some(&mut options))
        .context("git status failed")
}

fn head_commit(repo: &Repository) -> Result<Option<git2::Commit<'_>>> {
    match repo.head() {
        Ok(head) => Ok(Some(
            head.peel_to_commit()
                .context("failed to resolve HEAD commit")?,
        )),
        Err(err) if err.code() == ErrorCode::UnbornBranch || err.code() == ErrorCode::NotFound => {
            Ok(None)
        }
        Err(err) => Err(err).context("failed to read HEAD"),
    }
}

fn signature(repo: &Repository) -> Result<Signature<'_>> {
    repo.signature()
        .or_else(|_| Signature::now("keepbook", "keepbook@example.invalid"))
        .context("failed to create git signature")
}

fn callbacks(repo: &Repository, git_config: &GitConfig) -> Result<RemoteCallbacks<'static>> {
    let config = repo.config().context("failed to read git config")?;
    let configured_ssh_key_path = configured_ssh_key_path_for_credentials(git_config);
    let mut callbacks = RemoteCallbacks::new();
    configure_certificate_check(&mut callbacks);
    callbacks.credentials(move |url, username, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            if let Some(username) = username {
                if let Some(path) = configured_ssh_key_path.as_deref() {
                    if let Ok(cred) = Cred::ssh_key(username, None, path, None) {
                        return Ok(cred);
                    }
                }

                if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }

                for path in default_ssh_identity_paths() {
                    if let Ok(cred) = Cred::ssh_key(username, None, &path, None) {
                        return Ok(cred);
                    }
                }
            }
        }

        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            if let Ok(cred) = Cred::credential_helper(&config, url, username) {
                return Ok(cred);
            }
        }

        if allowed.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }

        Cred::credential_helper(&config, url, username)
    });
    Ok(callbacks)
}

fn configured_ssh_key_path_for_credentials(git_config: &GitConfig) -> Option<std::path::PathBuf> {
    git_config
        .ssh_key_path
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
}

#[cfg(target_os = "android")]
fn configure_certificate_check(callbacks: &mut RemoteCallbacks<'static>) {
    callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));
}

#[cfg(not(target_os = "android"))]
fn configure_certificate_check(_callbacks: &mut RemoteCallbacks<'static>) {}

fn fetch_remote(
    repo: &Repository,
    remote_name: &str,
    branches: &[&str],
    git_config: &GitConfig,
) -> Result<()> {
    let callbacks = callbacks(repo, git_config)?;
    let mut options = FetchOptions::new();
    options.remote_callbacks(callbacks);

    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("failed to find git remote {remote_name}"))?;
    remote
        .fetch(branches, Some(&mut options), None)
        .with_context(|| format!("failed to fetch git remote {remote_name}"))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeResult {
    UpToDate,
    Merged,
    Conflict,
}

fn merge_ref(repo: &Repository, ref_name: &str, label: &str) -> Result<MergeResult> {
    let reference = repo
        .find_reference(ref_name)
        .with_context(|| format!("failed to find git ref {ref_name}"))?;
    let annotated = repo
        .reference_to_annotated_commit(&reference)
        .with_context(|| format!("failed to resolve git ref {ref_name}"))?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .context("failed to analyze git merge")?;

    if analysis.is_up_to_date() {
        return Ok(MergeResult::UpToDate);
    }

    if analysis.is_fast_forward() {
        fast_forward(repo, annotated.id(), label)?;
        return Ok(MergeResult::Merged);
    }

    if !analysis.is_normal() {
        anyhow::bail!("git merge {label} is not supported by libgit2 analysis");
    }

    let ours = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .context("failed to resolve HEAD for merge")?;
    let theirs = repo
        .find_commit(annotated.id())
        .with_context(|| format!("failed to read git commit for {label}"))?;
    let merge_options = MergeOptions::new();
    let mut index = repo
        .merge_commits(&ours, &theirs, Some(&merge_options))
        .with_context(|| format!("git merge {label} failed"))?;

    if index.has_conflicts() {
        return Ok(MergeResult::Conflict);
    }

    let tree_id = index
        .write_tree_to(repo)
        .context("failed to write merge tree")?;
    let tree = repo
        .find_tree(tree_id)
        .context("failed to read merge tree")?;
    let signature = signature(repo)?;
    let message = format!("Merge {label}");

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&ours, &theirs],
    )
    .with_context(|| format!("failed to commit git merge {label}"))?;
    checkout_head(repo)?;
    Ok(MergeResult::Merged)
}

fn fast_forward(repo: &Repository, target: git2::Oid, label: &str) -> Result<()> {
    let head = repo.head().context("failed to read HEAD")?;
    if head.is_branch() {
        let name = head
            .name()
            .context("current git branch name is not UTF-8")?;
        let mut reference = repo
            .find_reference(name)
            .with_context(|| format!("failed to find current git branch {name}"))?;
        reference
            .set_target(target, &format!("Fast-forward to {label}"))
            .with_context(|| format!("failed to fast-forward current branch to {label}"))?;
        repo.set_head(name)
            .with_context(|| format!("failed to set HEAD after fast-forward to {label}"))?;
    } else {
        repo.set_head_detached(target)
            .with_context(|| format!("failed to set detached HEAD to {label}"))?;
    }
    checkout_head(repo)
}

fn checkout_head(repo: &Repository) -> Result<()> {
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .context("failed to checkout git HEAD")
}

fn upstream_tracking_ref(repo: &Repository) -> Result<Option<String>> {
    let head = repo.head().context("failed to read HEAD")?;
    let branch_name = head
        .shorthand()
        .context("current git branch is not UTF-8")?;
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .with_context(|| format!("failed to find current branch {branch_name}"))?;
    let upstream = match branch.upstream() {
        Ok(upstream) => upstream,
        Err(err) if err.code() == ErrorCode::NotFound || err.code() == ErrorCode::InvalidSpec => {
            return Ok(None);
        }
        Err(err) => return Err(err).context("failed to read upstream branch"),
    };
    Ok(Some(
        upstream
            .get()
            .name()
            .context("upstream git branch name is not UTF-8")?
            .to_string(),
    ))
}

fn parse_remote_tracking_ref(ref_name: &str) -> Option<(String, String)> {
    let rest = ref_name.strip_prefix("refs/remotes/")?;
    let (remote, branch) = rest.split_once('/')?;
    Some((remote.to_string(), branch.to_string()))
}

fn push_current_branch(repo: &Repository, git_config: &GitConfig) -> Result<()> {
    let head = repo.head().context("failed to read HEAD")?;
    let branch_name = head
        .shorthand()
        .context("current git branch name is not UTF-8")?;
    let upstream = upstream_tracking_ref(repo)?;
    let (remote_name, remote_branch) = upstream
        .as_deref()
        .and_then(parse_remote_tracking_ref)
        .unwrap_or_else(|| ("origin".to_string(), branch_name.to_string()));

    let callbacks = callbacks(repo, git_config)?;
    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks);
    let mut remote = repo
        .find_remote(&remote_name)
        .with_context(|| format!("failed to find git remote {remote_name}"))?;
    let refspec = format!("refs/heads/{branch_name}:refs/heads/{remote_branch}");
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .with_context(|| format!("failed to push git refspec {refspec}"))?;
    Ok(())
}

fn default_ssh_identity_paths() -> Vec<std::path::PathBuf> {
    let Some(home_dir) = dirs::home_dir() else {
        return Vec::new();
    };
    default_ssh_identity_paths_in_home(&home_dir)
}

fn default_ssh_identity_paths_in_home(home_dir: &Path) -> Vec<std::path::PathBuf> {
    let ssh_dir = home_dir.join(".ssh");
    DEFAULT_SSH_IDENTITY_FILES
        .iter()
        .map(|name| ssh_dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/git_tests.rs"]
mod git_tests;
