import { describe, it, expect, afterEach } from 'vitest';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { tryAutoCommit, tryMergeOriginMaster } from './git.js';

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

async function getCurrentBranch(dir: string): Promise<string> {
  const { stdout } = await execFileAsync('git', ['-C', dir, 'rev-parse', '--abbrev-ref', 'HEAD']);
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

  it('pushes to remote when autoPush is enabled', async () => {
    const dir = await makeTmpDir();
    const remote = await makeTmpDir();
    await execFileAsync('git', ['-C', remote, 'init', '--bare']);

    await initRepo(dir);
    await execFileAsync('git', ['-C', dir, 'remote', 'add', 'origin', remote]);

    // Create initial commit and establish upstream tracking branch.
    await fs.writeFile(path.join(dir, 'initial.txt'), 'initial');
    await execFileAsync('git', ['-C', dir, 'add', '-A']);
    await execFileAsync('git', ['-C', dir, 'commit', '-m', 'initial']);
    const branch = await getCurrentBranch(dir);
    await execFileAsync('git', ['-C', dir, 'push', '-u', 'origin', branch]);

    await fs.writeFile(path.join(dir, 'data.txt'), 'data');

    const result = await tryAutoCommit(dir, 'sync mock', true);
    expect(result).toEqual({ type: 'committed' });

    const { stdout } = await execFileAsync('git', ['-C', remote, 'log', '-1', '--format=%s']);
    expect(stdout.trim()).toBe('keepbook: sync mock');
  });
});

describe('tryMergeOriginMaster', () => {
  const tmpDirs: string[] = [];

  async function makeTmpDir(): Promise<string> {
    const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-git-'));
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
    const result = await tryMergeOriginMaster(dir);
    expect(result).toEqual({
      type: 'skipped_not_repo',
      reason: 'data directory is not a git repository',
    });
  });

  it('merges remote master fast-forward', async () => {
    const remote = await makeTmpDir();
    await execFileAsync('git', ['-C', remote, 'init', '--bare']);

    const source = await makeTmpDir();
    await initRepo(source);
    await execFileAsync('git', ['-C', source, 'remote', 'add', 'origin', remote]);
    await fs.writeFile(path.join(source, 'file.txt'), 'base\n');
    await execFileAsync('git', ['-C', source, 'add', '-A']);
    await execFileAsync('git', ['-C', source, 'commit', '-m', 'base']);
    const branch = await getCurrentBranch(source);
    await execFileAsync('git', ['-C', source, 'push', '-u', 'origin', branch]);

    const local = await makeTmpDir();
    await execFileAsync('git', ['-C', local, 'clone', remote, '.']);
    await execFileAsync('git', ['-C', local, 'config', 'user.email', 'test@example.com']);
    await execFileAsync('git', ['-C', local, 'config', 'user.name', 'Keepbook Test']);

    await fs.writeFile(path.join(source, 'file.txt'), 'base\nremote\n');
    await execFileAsync('git', ['-C', source, 'add', '-A']);
    await execFileAsync('git', ['-C', source, 'commit', '-m', 'remote update']);
    await execFileAsync('git', ['-C', source, 'push']);

    const result = await tryMergeOriginMaster(local);
    expect(result).toEqual({ type: 'merged' });

    const content = await fs.readFile(path.join(local, 'file.txt'), 'utf-8');
    expect(content).toContain('remote');
  });

  it('aborts and reports conflicts', async () => {
    const remote = await makeTmpDir();
    await execFileAsync('git', ['-C', remote, 'init', '--bare']);

    const source = await makeTmpDir();
    await initRepo(source);
    await execFileAsync('git', ['-C', source, 'remote', 'add', 'origin', remote]);
    await fs.writeFile(path.join(source, 'conflict.txt'), 'line\n');
    await execFileAsync('git', ['-C', source, 'add', '-A']);
    await execFileAsync('git', ['-C', source, 'commit', '-m', 'base']);
    const branch = await getCurrentBranch(source);
    await execFileAsync('git', ['-C', source, 'push', '-u', 'origin', branch]);

    const local = await makeTmpDir();
    await execFileAsync('git', ['-C', local, 'clone', remote, '.']);
    await execFileAsync('git', ['-C', local, 'config', 'user.email', 'test@example.com']);
    await execFileAsync('git', ['-C', local, 'config', 'user.name', 'Keepbook Test']);
    await execFileAsync('git', ['-C', local, 'checkout', '-b', 'work']);

    await fs.writeFile(path.join(local, 'conflict.txt'), 'local\n');
    await execFileAsync('git', ['-C', local, 'add', '-A']);
    await execFileAsync('git', ['-C', local, 'commit', '-m', 'local change']);

    await fs.writeFile(path.join(source, 'conflict.txt'), 'remote\n');
    await execFileAsync('git', ['-C', source, 'add', '-A']);
    await execFileAsync('git', ['-C', source, 'commit', '-m', 'remote change']);
    await execFileAsync('git', ['-C', source, 'push']);

    const result = await tryMergeOriginMaster(local);
    expect(result).toEqual({ type: 'conflict_aborted' });

    const content = await fs.readFile(path.join(local, 'conflict.txt'), 'utf-8');
    expect(content).toBe('local\n');
  });
});
