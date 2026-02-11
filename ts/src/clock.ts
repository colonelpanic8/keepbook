/**
 * Clock abstraction (port of Rust clock.rs).
 *
 * - `now()` returns the current instant as a `Date` (UTC).
 * - `today()` returns the current UTC date as a "YYYY-MM-DD" string.
 */
export interface Clock {
  now(): Date;
  today(): string;
}

/** Real wall-clock backed by `Date.now()`. */
export class SystemClock implements Clock {
  now(): Date {
    return new Date();
  }

  today(): string {
    return this.now().toISOString().slice(0, 10);
  }
}

/** Clock frozen at a specific instant. Useful for deterministic tests. */
export class FixedClock implements Clock {
  private readonly _now: Date;

  constructor(now: Date) {
    this._now = new Date(now.getTime());
  }

  now(): Date {
    return new Date(this._now.getTime());
  }

  today(): string {
    return this._now.toISOString().slice(0, 10);
  }
}
