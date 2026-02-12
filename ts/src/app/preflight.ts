import type { ResolvedConfig } from '../config.js';
import { tryMergeOriginMaster } from '../git.js';

export interface PreflightOptions {
  merge_origin_master: boolean;
}

/**
 * Pre-command hook for CLI/frontends.
 *
 * Intentionally decoupled from CLI parsing: callers compute enablement.
 */
export async function runPreflight(config: ResolvedConfig, opts: PreflightOptions): Promise<void> {
  if (!opts.merge_origin_master) {
    return;
  }

  const outcome = await tryMergeOriginMaster(config.data_dir);
  if (outcome.type === 'conflict_aborted') {
    throw new Error('Preflight git merge aborted due to conflicts (origin/master)');
  }
}

