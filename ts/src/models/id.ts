import { v4 as uuidv4, v5 as uuidv5 } from 'uuid';

/**
 * Error thrown when an Id value fails path-safety validation.
 */
export class IdError extends Error {
  public readonly value: string;

  constructor(value: string) {
    super(
      `Invalid id ${JSON.stringify(value)}: ids must be a single path segment (no '/', '\\', NUL, '.' or '..')`,
    );
    this.name = 'IdError';
    this.value = value;
  }
}

/**
 * Opaque identifier for stored entities.
 *
 * For file-backed storage, ids should be safe path segments (no slashes).
 * Serializes transparently as a plain string in JSON.
 */
export class Id {
  /** Namespace UUID for generating deterministic IDs from external identifiers. */
  private static readonly NAMESPACE = '6ba7b810-9dad-11d1-80b4-00c04fd430c8';

  private readonly inner: string;

  private constructor(value: string) {
    this.inner = value;
  }

  /** Create a new random Id (UUID v4). */
  static new(): Id {
    return new Id(uuidv4());
  }

  /** Create an Id from an arbitrary string (no validation). */
  static fromString(value: string): Id {
    return new Id(value);
  }

  /**
   * Create an Id from a string, validating that it is a safe path segment.
   * Throws IdError if the value is not path-safe.
   */
  static fromStringChecked(value: string): Id {
    if (Id.isPathSafe(value)) {
      return new Id(value);
    }
    throw new IdError(value);
  }

  /**
   * Create a deterministic, filesystem-safe Id from an external identifier.
   * Uses UUID v5 with a fixed namespace, so the same input always produces the same Id.
   * Useful for external IDs that may contain special characters (like base64).
   */
  static fromExternal(value: string): Id {
    return new Id(uuidv5(value, Id.NAMESPACE));
  }

  /** Returns true if the string is safe to use as a single path segment. */
  static isPathSafe(value: string): boolean {
    if (value === '' || value === '.' || value === '..') {
      return false;
    }
    for (let i = 0; i < value.length; i++) {
      const ch = value[i];
      if (ch === '/' || ch === '\\' || ch === '\0') {
        return false;
      }
    }
    return true;
  }

  /** The underlying string value. */
  asStr(): string {
    return this.inner;
  }

  /** Value equality comparison. */
  equals(other: Id): boolean {
    return this.inner === other.inner;
  }

  /** String coercion returns the inner value. */
  toString(): string {
    return this.inner;
  }

  /** JSON serialization returns the plain string (transparent). */
  toJSON(): string {
    return this.inner;
  }
}
