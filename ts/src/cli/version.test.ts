import { describe, expect, it } from 'vitest';

import { cliVersion, packageVersion } from './version.js';

describe('cliVersion', () => {
  it('uses the package version instead of a hard-coded stale value', () => {
    expect(packageVersion()).toBe('0.2.0');
  });

  it('matches the Rust CLI version shape', () => {
    expect(cliVersion()).toMatch(
      /^(keepbook 0\.2\.0 \(git commit [0-9a-f]{40}\)|keepbook 0\.2\.0 \(git commit unknown\))$/,
    );
  });
});
