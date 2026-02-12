import { describe, it, expect } from 'vitest';

import { parseExportedSession } from './schwab.js';

describe('schwab session parsing', () => {
  it('strips Bearer prefix from exported token', () => {
    const session = parseExportedSession(JSON.stringify({ token: 'Bearer test-token', cookies: {} }));
    expect(session.token).toBe('test-token');
  });
});

