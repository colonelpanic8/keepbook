use anyhow::Result;

use crate::config::ResolvedConfig;
use crate::git::{try_merge_origin_master, MergeOriginMasterOutcome};

#[derive(Debug, Clone, Copy, Default)]
pub struct PreflightOptions {
    pub merge_origin_master: bool,
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

    Ok(())
}
