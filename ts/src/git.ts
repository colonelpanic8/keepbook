import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { realpath } from 'node:fs/promises';
import { resolve } from 'node:path';

const execFileAsync = promisify(execFile);

export type AutoCommitOutcome =
  | { type: 'skipped_not_repo'; reason: string }
  | { type: 'skipped_no_changes' }
  | { type: 'committed' };

export type MergeOriginMasterOutcome =
  | { type: 'skipped_not_repo'; reason: string }
  | { type: 'up_to_date' }
  | { type: 'merged' }
  | { type: 'conflict_aborted' };

export async function tryAutoCommit(
  dataDir: string,
  action: string,
  autoPush = false,
): Promise<AutoCommitOutcome> {
  // 1. Find git repo root
  const repoRoot = await gitRepoRoot(dataDir);
  if (repoRoot === null) {
    return { type: 'skipped_not_repo', reason: 'data directory is not a git repository' };
  }

  // 2. Canonicalize both paths and compare
  const canonicalRoot = await canonicalize(repoRoot);
  const canonicalDir = await canonicalize(dataDir);

  if (canonicalRoot !== canonicalDir) {
    return {
      type: 'skipped_not_repo',
      reason: `data directory is not the git repo root (repo root: ${canonicalRoot})`,
    };
  }

  // 3. Check status
  const { stdout: statusOut } = await gitOutput(canonicalDir, ['status', '--porcelain']);
  if (statusOut.trim() === '') {
    return { type: 'skipped_no_changes' };
  }

  // 4. git add -A
  await gitOutput(canonicalDir, ['add', '-A']);

  // 5. git commit
  const trimmedAction = action.trim();
  const message = trimmedAction === '' ? 'keepbook: update data' : `keepbook: ${trimmedAction}`;
  await gitOutput(canonicalDir, ['commit', '-m', message]);

  // 6. git push
  if (autoPush) {
    await gitOutput(canonicalDir, ['push']);
  }

  return { type: 'committed' };
}

export async function tryMergeOriginMaster(dataDir: string): Promise<MergeOriginMasterOutcome> {
  const repoRoot = await gitRepoRoot(dataDir);
  if (repoRoot === null) {
    return { type: 'skipped_not_repo', reason: 'data directory is not a git repository' };
  }

  const canonicalRoot = await canonicalize(repoRoot);
  const canonicalDir = await canonicalize(dataDir);

  if (canonicalRoot !== canonicalDir) {
    return {
      type: 'skipped_not_repo',
      reason: `data directory is not the git repo root (repo root: ${canonicalRoot})`,
    };
  }

  // Refuse to merge with a dirty worktree; it can either fail or create hard-to-debug state.
  const { stdout: statusOut } = await gitOutput(canonicalDir, ['status', '--porcelain']);
  if (statusOut.trim() !== '') {
    throw new Error('git working tree is not clean; cannot merge origin/master');
  }

  const { stdout: headBefore } = await gitOutput(canonicalDir, ['rev-parse', 'HEAD']);

  await gitOutput(canonicalDir, ['fetch', 'origin', 'master']);

  try {
    await gitOutput(canonicalDir, ['merge', '--no-edit', 'origin/master']);
  } catch (err) {
    const { stdout: unmergedOut } = await gitOutput(canonicalDir, [
      'diff',
      '--name-only',
      '--diff-filter=U',
    ]);
    if (unmergedOut.trim() !== '') {
      await gitOutput(canonicalDir, ['merge', '--abort']);
      return { type: 'conflict_aborted' };
    }
    throw err;
  }

  const { stdout: headAfter } = await gitOutput(canonicalDir, ['rev-parse', 'HEAD']);
  if (headBefore.trim() === headAfter.trim()) {
    return { type: 'up_to_date' };
  }
  return { type: 'merged' };
}

async function canonicalize(p: string): Promise<string> {
  try {
    return await realpath(p);
  } catch {
    return resolve(p);
  }
}

async function gitRepoRoot(dir: string): Promise<string | null> {
  try {
    const { stdout } = await execFileAsync('git', ['-C', dir, 'rev-parse', '--show-toplevel']);
    const root = stdout.trim();
    return root === '' ? null : root;
  } catch {
    return null;
  }
}

async function gitOutput(dir: string, args: string[]): Promise<{ stdout: string; stderr: string }> {
  try {
    return await execFileAsync('git', ['-C', dir, ...args]);
  } catch (err: unknown) {
    if (err && typeof err === 'object' && 'code' in err && err.code === 'ENOENT') {
      throw new Error('git not found in PATH');
    }
    throw err;
  }
}
