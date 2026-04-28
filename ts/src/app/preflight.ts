import type { ResolvedConfig } from '../config.js';
import { tryMergeOriginMaster, tryPullRemote } from '../git.js';

export interface PreflightOptions {
  merge_origin_master: boolean;
  pull_remote: boolean;
}

/**
 * Pre-command hook for CLI/frontends.
 *
 * Intentionally decoupled from CLI parsing: callers compute enablement.
 */
export async function runPreflight(config: ResolvedConfig, opts: PreflightOptions): Promise<void> {
  if (!opts.merge_origin_master) {
    if (!opts.pull_remote) return;
  } else {
    const outcome = await tryMergeOriginMaster(config.data_dir);
    if (outcome.type === 'conflict_aborted') {
      throw new Error('Preflight git merge aborted due to conflicts (origin/master)');
    }
  }

  if (opts.pull_remote) {
    const outcome = await tryPullRemote(config.data_dir);
    if (outcome.type === 'conflict_aborted') {
      throw new Error('Preflight git pull aborted due to conflicts');
    }
  }
}
