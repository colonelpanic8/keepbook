import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

type PackageJson = {
  version?: unknown;
};

export function packageVersion(): string {
  const packageJsonUrl = new URL('../../package.json', import.meta.url);
  const parsed = JSON.parse(readFileSync(packageJsonUrl, 'utf8')) as PackageJson;
  if (typeof parsed.version !== 'string' || parsed.version.trim() === '') {
    return 'unknown';
  }
  return parsed.version.trim();
}

export function gitCommitHash(): string {
  try {
    const repoDir = new URL('../..', import.meta.url);
    return (
      execFileSync('git', ['rev-parse', 'HEAD'], {
        cwd: repoDir,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'ignore'],
      }).trim() || 'unknown'
    );
  } catch {
    return 'unknown';
  }
}

export function cliVersion(): string {
  return `keepbook ${packageVersion()} (git commit ${gitCommitHash()})`;
}
