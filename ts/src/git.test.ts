import { describe, it, expect, afterEach } from 'vitest';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { tryAutoCommit } from './git.js';

const execFileAsync = promisify(execFile);

async function initRepo(dir: string): Promise<void> {
  await execFileAsync('git', ['-C', dir, 'init']);
  await execFileAsync('git', ['-C', dir, 'config', 'user.email', 'test@example.com']);
  await execFileAsync('git', ['-C', dir, 'config', 'user.name', 'Keepbook Test']);
}

async function getLastCommitMessage(dir: string): Promise<string> {
  const { stdout } = await execFileAsync('git', ['-C', dir, 'log', '-1', '--format=%s']);
  return stdout.trim();
}

describe('tryAutoCommit', () => {
  const tmpDirs: string[] = [];

  async function makeTmpDir(): Promise<string> {
    const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-git-'));
    // Resolve to canonical path so comparisons work (e.g. /tmp -> /private/tmp on macOS)
    const resolved = await fs.realpath(dir);
    tmpDirs.push(resolved);
    return resolved;
  }

  afterEach(async () => {
    for (const dir of tmpDirs) {
      await fs.rm(dir, { recursive: true, force: true });
    }
    tmpDirs.length = 0;
  });

  it('skips when not a git repo', async () => {
    const dir = await makeTmpDir();
    const result = await tryAutoCommit(dir, 'test action');

    expect(result).toEqual({
      type: 'skipped_not_repo',
      reason: 'data directory is not a git repository',
    });
  });

  it('skips when repo root does not match data dir', async () => {
    const dir = await makeTmpDir();
    await initRepo(dir);

    const subdir = path.join(dir, 'subdir');
    await fs.mkdir(subdir);

    const result = await tryAutoCommit(subdir, 'test action');

    expect(result.type).toBe('skipped_not_repo');
    expect(result).toHaveProperty('reason');
    const reason = (result as { type: 'skipped_not_repo'; reason: string }).reason;
    expect(reason).toContain('data directory is not the git repo root');
    expect(reason).toContain(dir);
  });

  it('commits changes with the given action', async () => {
    const dir = await makeTmpDir();
    await initRepo(dir);

    // Create an initial commit so HEAD exists
    await fs.writeFile(path.join(dir, 'initial.txt'), 'initial');
    await execFileAsync('git', ['-C', dir, 'add', '-A']);
    await execFileAsync('git', ['-C', dir, 'commit', '-m', 'initial']);

    // Create a new file to trigger changes
    await fs.writeFile(path.join(dir, 'data.txt'), 'some data');

    const result = await tryAutoCommit(dir, 'import transactions');

    expect(result).toEqual({ type: 'committed' });

    const message = await getLastCommitMessage(dir);
    expect(message).toBe('keepbook: import transactions');
  });

  it('skips when there are no changes', async () => {
    const dir = await makeTmpDir();
    await initRepo(dir);

    // Make an initial commit so the repo is clean
    await fs.writeFile(path.join(dir, 'initial.txt'), 'initial');
    await execFileAsync('git', ['-C', dir, 'add', '-A']);
    await execFileAsync('git', ['-C', dir, 'commit', '-m', 'initial']);

    const result = await tryAutoCommit(dir, 'some action');

    expect(result).toEqual({ type: 'skipped_no_changes' });
  });

  it('uses default message when action is empty', async () => {
    const dir = await makeTmpDir();
    await initRepo(dir);

    // Create initial commit
    await fs.writeFile(path.join(dir, 'initial.txt'), 'initial');
    await execFileAsync('git', ['-C', dir, 'add', '-A']);
    await execFileAsync('git', ['-C', dir, 'commit', '-m', 'initial']);

    // Create a new file
    await fs.writeFile(path.join(dir, 'data.txt'), 'data');

    const result = await tryAutoCommit(dir, '');

    expect(result).toEqual({ type: 'committed' });

    const message = await getLastCommitMessage(dir);
    expect(message).toBe('keepbook: update data');
  });

  it('uses default message when action is whitespace-only', async () => {
    const dir = await makeTmpDir();
    await initRepo(dir);

    // Create initial commit
    await fs.writeFile(path.join(dir, 'initial.txt'), 'initial');
    await execFileAsync('git', ['-C', dir, 'add', '-A']);
    await execFileAsync('git', ['-C', dir, 'commit', '-m', 'initial']);

    // Create a new file
    await fs.writeFile(path.join(dir, 'data.txt'), 'data');

    const result = await tryAutoCommit(dir, '   ');

    expect(result).toEqual({ type: 'committed' });

    const message = await getLastCommitMessage(dir);
    expect(message).toBe('keepbook: update data');
  });
});
