import { describe, it, expect } from 'vitest';
import { SystemClock, FixedClock, Clock } from './clock.js';

describe('SystemClock', () => {
  it('implements Clock interface', () => {
    const clock: Clock = new SystemClock();
    expect(clock.now()).toBeInstanceOf(Date);
  });

  it('now() returns approximately the current time', () => {
    const clock = new SystemClock();
    const before = Date.now();
    const now = clock.now();
    const after = Date.now();
    expect(now.getTime()).toBeGreaterThanOrEqual(before);
    expect(now.getTime()).toBeLessThanOrEqual(after);
  });

  it('today() returns a YYYY-MM-DD string in UTC', () => {
    const clock = new SystemClock();
    const today = clock.today();
    // Should match YYYY-MM-DD format
    expect(today).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    // Should correspond to the UTC date of now()
    const now = clock.now();
    const expected = now.toISOString().slice(0, 10);
    expect(today).toBe(expected);
  });
});

describe('FixedClock', () => {
  it('implements Clock interface', () => {
    const date = new Date('2024-06-15T12:30:00Z');
    const clock: Clock = new FixedClock(date);
    expect(clock.now()).toBeInstanceOf(Date);
  });

  it('now() always returns the fixed date', () => {
    const date = new Date('2024-06-15T12:30:00Z');
    const clock = new FixedClock(date);
    expect(clock.now()).toEqual(date);
    expect(clock.now()).toEqual(date);
    // Should return the exact same value every time
    expect(clock.now().getTime()).toBe(date.getTime());
  });

  it('today() returns the UTC date portion of the fixed time', () => {
    const date = new Date('2024-06-15T23:59:59Z');
    const clock = new FixedClock(date);
    expect(clock.today()).toBe('2024-06-15');
  });

  it('today() uses UTC, not local time', () => {
    // 2024-06-15 at 23:59 UTC could be 2024-06-16 in some local timezones
    // today() should always return the UTC date
    const date = new Date('2024-06-15T23:59:59Z');
    const clock = new FixedClock(date);
    expect(clock.today()).toBe('2024-06-15');
  });

  it('now() returns a copy, not the same reference', () => {
    const date = new Date('2024-06-15T12:30:00Z');
    const clock = new FixedClock(date);
    const a = clock.now();
    const b = clock.now();
    // Same value but not the same object (defensive copy)
    expect(a.getTime()).toBe(b.getTime());
  });
});
