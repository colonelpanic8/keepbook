import { describe, it, expect } from 'vitest';
import { Id, IdError } from './id.js';

describe('Id', () => {
  describe('new', () => {
    it('generates a valid UUID v4 string', () => {
      const id = Id.new();
      // UUID v4 format: 8-4-4-4-12 hex chars
      expect(id.asStr()).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      );
    });

    it('generates unique ids each time', () => {
      const a = Id.new();
      const b = Id.new();
      expect(a.equals(b)).toBe(false);
    });
  });

  describe('fromString', () => {
    it('keeps the given value', () => {
      const id = Id.fromString('account-id-123');
      expect(id.asStr()).toBe('account-id-123');
    });

    it('accepts arbitrary strings without validation', () => {
      // fromString does NOT validate -- that is fromStringChecked's job
      const id = Id.fromString('foo/bar');
      expect(id.asStr()).toBe('foo/bar');
    });
  });

  describe('fromStringChecked', () => {
    it('accepts a safe path segment', () => {
      const id = Id.fromStringChecked('valid-id');
      expect(id.asStr()).toBe('valid-id');
    });

    it('rejects empty string', () => {
      expect(() => Id.fromStringChecked('')).toThrow(IdError);
    });

    it('rejects "."', () => {
      expect(() => Id.fromStringChecked('.')).toThrow(IdError);
    });

    it('rejects ".."', () => {
      expect(() => Id.fromStringChecked('..')).toThrow(IdError);
    });

    it('rejects "../escape"', () => {
      expect(() => Id.fromStringChecked('../escape')).toThrow(IdError);
    });

    it('rejects strings containing "/"', () => {
      expect(() => Id.fromStringChecked('foo/bar')).toThrow(IdError);
    });

    it('rejects strings containing "\\"', () => {
      expect(() => Id.fromStringChecked('foo\\bar')).toThrow(IdError);
    });

    it('rejects strings containing NUL', () => {
      expect(() => Id.fromStringChecked('bad\0id')).toThrow(IdError);
    });

    it('includes the offending value in the error message', () => {
      try {
        Id.fromStringChecked('foo/bar');
        expect.unreachable('should have thrown');
      } catch (e) {
        expect(e).toBeInstanceOf(IdError);
        expect((e as IdError).message).toContain('foo/bar');
      }
    });
  });

  describe('fromExternal', () => {
    it('is deterministic for the same input', () => {
      const first = Id.fromExternal('schwab-account-123');
      const second = Id.fromExternal('schwab-account-123');
      expect(first.equals(second)).toBe(true);
      expect(first.asStr()).toBe(second.asStr());
    });

    it('differs for different inputs', () => {
      const first = Id.fromExternal('schwab-account-123');
      const second = Id.fromExternal('schwab-account-456');
      expect(first.equals(second)).toBe(false);
    });

    it('produces a path-safe result', () => {
      const id = Id.fromExternal('weird/account/value');
      expect(id.asStr()).not.toContain('/');
      expect(Id.isPathSafe(id.asStr())).toBe(true);
    });

    it('produces a valid UUID string', () => {
      const id = Id.fromExternal('some-external-id');
      expect(id.asStr()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    });
  });

  describe('isPathSafe', () => {
    it('returns false for empty string', () => {
      expect(Id.isPathSafe('')).toBe(false);
    });

    it('returns false for "."', () => {
      expect(Id.isPathSafe('.')).toBe(false);
    });

    it('returns false for ".."', () => {
      expect(Id.isPathSafe('..')).toBe(false);
    });

    it('returns false for strings containing "/"', () => {
      expect(Id.isPathSafe('foo/bar')).toBe(false);
    });

    it('returns false for strings containing "\\"', () => {
      expect(Id.isPathSafe('foo\\bar')).toBe(false);
    });

    it('returns false for strings containing NUL', () => {
      expect(Id.isPathSafe('foo\0bar')).toBe(false);
    });

    it('returns true for normal strings', () => {
      expect(Id.isPathSafe('valid-id')).toBe(true);
      expect(Id.isPathSafe('account_123')).toBe(true);
      expect(Id.isPathSafe('hello.world')).toBe(true);
    });
  });

  describe('equals', () => {
    it('returns true for identical values', () => {
      const a = Id.fromString('abc');
      const b = Id.fromString('abc');
      expect(a.equals(b)).toBe(true);
    });

    it('returns false for different values', () => {
      const a = Id.fromString('abc');
      const b = Id.fromString('def');
      expect(a.equals(b)).toBe(false);
    });
  });

  describe('toString', () => {
    it('returns the inner string', () => {
      const id = Id.fromString('my-id');
      expect(id.toString()).toBe('my-id');
      expect(`${id}`).toBe('my-id');
    });
  });

  describe('toJSON', () => {
    it('serializes as a plain string (transparent)', () => {
      const id = Id.fromString('my-id');
      expect(JSON.stringify(id)).toBe('"my-id"');
    });

    it('serializes correctly inside an object', () => {
      const obj = { id: Id.fromString('abc-123') };
      expect(JSON.parse(JSON.stringify(obj))).toEqual({ id: 'abc-123' });
    });
  });
});
