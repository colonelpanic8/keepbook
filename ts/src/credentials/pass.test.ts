import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

import { PassCredentialStore } from './pass.js';

describe('PassCredentialStore', () => {
  let tmpDir: string;
  let binDir: string;
  let oldPath: string | undefined;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-pass-test-'));
    binDir = path.join(tmpDir, 'bin');
    await fs.mkdir(binDir, { recursive: true });

    oldPath = process.env.PATH;
    process.env.PATH = `${binDir}:${oldPath ?? ''}`;
  });

  afterEach(async () => {
    if (oldPath !== undefined) process.env.PATH = oldPath;
    await fs.rm(tmpDir, { recursive: true, force: true });
  });

  it('parses pass show output (password + field lines with \\\\n unescape)', async () => {
    const passScript = [
      '#!/usr/bin/env bash',
      'set -euo pipefail',
      'if [[ "$1" != "show" ]]; then',
      '  echo "unsupported" >&2',
      '  exit 2',
      'fi',
      'cat <<EOF',
      'pwline',
      'key-name: organizations/abc',
      'private-key: -----BEGIN EC PRIVATE KEY-----\\\\nLINE1\\\\nLINE2\\\\n-----END EC PRIVATE KEY-----',
      'EOF',
      '',
    ].join('\n');
    const passPath = path.join(binDir, 'pass');
    await fs.writeFile(passPath, passScript, { encoding: 'utf8' });
    fsSync.chmodSync(passPath, 0o755);

    const store = new PassCredentialStore({
      backend: 'pass',
      path: 'finance/coinbase-api',
      fields: {
        key_name: 'key-name',
        private_key: 'private-key',
      },
    });

    expect(await store.get('password')).toBe('pwline');
    expect(await store.get('key_name')).toBe('organizations/abc');
    const pk = await store.get('private_key');
    expect(pk).not.toBeNull();
    expect(pk!).toContain('BEGIN EC PRIVATE KEY');
    expect(pk!).toContain('\nLINE1\n');
  });
});

