import { describe, expect, it } from 'vitest';
import { parseDuration, formatDuration } from './duration.js';

describe('parseDuration', () => {
  describe('days', () => {
    it('parses 1d', () => {
      expect(parseDuration('1d')).toBe(86400_000);
    });

    it('parses 14d', () => {
      expect(parseDuration('14d')).toBe(14 * 86400_000);
    });

    it('parses 365d', () => {
      expect(parseDuration('365d')).toBe(365 * 86400_000);
    });
  });

  describe('hours', () => {
    it('parses 1h', () => {
      expect(parseDuration('1h')).toBe(3600_000);
    });

    it('parses 24h', () => {
      expect(parseDuration('24h')).toBe(24 * 3600_000);
    });

    it('parses 48h', () => {
      expect(parseDuration('48h')).toBe(48 * 3600_000);
    });
  });

  describe('minutes', () => {
    it('parses 1m', () => {
      expect(parseDuration('1m')).toBe(60_000);
    });

    it('parses 30m', () => {
      expect(parseDuration('30m')).toBe(30 * 60_000);
    });

    it('parses 90m', () => {
      expect(parseDuration('90m')).toBe(90 * 60_000);
    });
  });

  describe('seconds', () => {
    it('parses 1s', () => {
      expect(parseDuration('1s')).toBe(1_000);
    });

    it('parses 60s', () => {
      expect(parseDuration('60s')).toBe(60_000);
    });

    it('parses 3600s', () => {
      expect(parseDuration('3600s')).toBe(3600_000);
    });
  });

  describe('case insensitive', () => {
    it('parses uppercase D', () => {
      expect(parseDuration('1D')).toBe(86400_000);
    });

    it('parses uppercase H', () => {
      expect(parseDuration('1H')).toBe(3600_000);
    });

    it('parses uppercase M', () => {
      expect(parseDuration('1M')).toBe(60_000);
    });

    it('parses uppercase S', () => {
      expect(parseDuration('1S')).toBe(1_000);
    });
  });

  describe('whitespace handling', () => {
    it('trims leading and trailing spaces', () => {
      expect(parseDuration('  1d  ')).toBe(86400_000);
    });

    it('trims tabs and newlines', () => {
      expect(parseDuration('\t24h\n')).toBe(24 * 3600_000);
    });

    it('trims mixed whitespace', () => {
      expect(parseDuration(' 30m ')).toBe(30 * 60_000);
    });
  });

  describe('invalid unit', () => {
    it('rejects unknown unit x', () => {
      expect(() => parseDuration('1x')).toThrow();
    });

    it('rejects unknown unit w', () => {
      expect(() => parseDuration('1w')).toThrow();
    });

    it('rejects bare number', () => {
      expect(() => parseDuration('1')).toThrow();
    });

    it('rejects bare unit', () => {
      expect(() => parseDuration('d')).toThrow();
    });
  });

  describe('invalid number', () => {
    it('rejects alphabetic input', () => {
      expect(() => parseDuration('abcd')).toThrow();
    });

    it('rejects negative number', () => {
      expect(() => parseDuration('-1d')).toThrow();
    });

    it('rejects float', () => {
      expect(() => parseDuration('1.5h')).toThrow();
    });
  });

  describe('empty input', () => {
    it('rejects empty string', () => {
      expect(() => parseDuration('')).toThrow();
    });

    it('rejects whitespace-only string', () => {
      expect(() => parseDuration('   ')).toThrow();
    });
  });

  describe('zero values', () => {
    it('parses 0d', () => {
      expect(parseDuration('0d')).toBe(0);
    });

    it('parses 0s', () => {
      expect(parseDuration('0s')).toBe(0);
    });
  });
});

describe('formatDuration', () => {
  describe('days', () => {
    it('formats 1 day', () => {
      expect(formatDuration(86400_000)).toBe('1d');
    });

    it('formats 14 days', () => {
      expect(formatDuration(14 * 86400_000)).toBe('14d');
    });
  });

  describe('hours', () => {
    it('formats 1 hour', () => {
      expect(formatDuration(3600_000)).toBe('1h');
    });

    it('formats 12 hours', () => {
      expect(formatDuration(12 * 3600_000)).toBe('12h');
    });
  });

  describe('minutes', () => {
    it('formats 1 minute', () => {
      expect(formatDuration(60_000)).toBe('1m');
    });

    it('formats 30 minutes', () => {
      expect(formatDuration(30 * 60_000)).toBe('30m');
    });
  });

  describe('seconds', () => {
    it('formats 1 second', () => {
      expect(formatDuration(1_000)).toBe('1s');
    });

    it('formats 45 seconds', () => {
      expect(formatDuration(45_000)).toBe('45s');
    });
  });

  describe('zero', () => {
    it('formats zero as 0s', () => {
      expect(formatDuration(0)).toBe('0s');
    });
  });

  describe('non-divisible durations fall back to seconds', () => {
    it('formats 90 seconds as 90s (not 1m 30s)', () => {
      expect(formatDuration(90_000)).toBe('90s');
    });

    it('formats 3700 seconds as 3700s', () => {
      expect(formatDuration(3700_000)).toBe('3700s');
    });
  });

  describe('prefers largest fitting unit', () => {
    it('24h formats as 1d, not 24h', () => {
      expect(formatDuration(24 * 3600_000)).toBe('1d');
    });

    it('60m formats as 1h, not 60m', () => {
      expect(formatDuration(60 * 60_000)).toBe('1h');
    });

    it('60s formats as 1m, not 60s', () => {
      expect(formatDuration(60 * 1_000)).toBe('1m');
    });
  });
});

describe('roundtrip', () => {
  const testCases = [
    86400_000, // 1d
    14 * 86400_000, // 14d
    3600_000, // 1h
    24 * 3600_000, // 24h (= 1d)
    60_000, // 1m
    30 * 60_000, // 30m
    1_000, // 1s
    45_000, // 45s
  ];

  for (const ms of testCases) {
    it(`roundtrips ${ms}ms`, () => {
      expect(parseDuration(formatDuration(ms))).toBe(ms);
    });
  }
});
