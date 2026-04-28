use anyhow::Result;

use crate::config::ResolvedConfig;
use crate::git::{
    try_merge_origin_master, try_pull_remote, MergeOriginMasterOutcome, PullRemoteOutcome,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct PreflightOptions {
    pub merge_origin_master: bool,
    pub pull_remote: bool,
}

/// Pre-command hook for CLI/frontends.
///
/// This is intentionally decoupled from CLI argument parsing; callers decide
/// when and whether to enable specific preflight steps.
pub fn run_preflight(config: &ResolvedConfig, opts: PreflightOptions) -> Result<()> {
    if opts.merge_origin_master {
        match try_merge_origin_master(&config.data_dir)? {
            MergeOriginMasterOutcome::SkippedNotRepo { reason } => {
                tracing::warn!("Preflight git merge skipped: {reason}");
            }
            MergeOriginMasterOutcome::UpToDate => {
                tracing::debug!("Preflight git merge: already up to date");
            }
            MergeOriginMasterOutcome::Merged => {
                tracing::info!("Preflight git merge: merged origin/master");
            }
            MergeOriginMasterOutcome::ConflictAborted => {
                anyhow::bail!("Preflight git merge aborted due to conflicts (origin/master)");
            }
        }
    }

    if opts.pull_remote {
        match try_pull_remote(&config.data_dir)? {
            PullRemoteOutcome::SkippedNotRepo { reason } => {
                tracing::warn!("Preflight git pull skipped: {reason}");
            }
            PullRemoteOutcome::SkippedNoUpstream { reason } => {
                tracing::warn!("Preflight git pull skipped: {reason}");
            }
            PullRemoteOutcome::UpToDate => {
                tracing::debug!("Preflight git pull: already up to date");
            }
            PullRemoteOutcome::Pulled => {
                tracing::info!("Preflight git pull: pulled remote changes");
            }
            PullRemoteOutcome::ConflictAborted => {
                anyhow::bail!("Preflight git pull aborted due to conflicts");
            }
        }
    }

    Ok(())
}
