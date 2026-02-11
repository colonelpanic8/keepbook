import { describe, it, expect } from 'vitest';
import { IdGenerator, UuidIdGenerator, FixedIdGenerator } from './id-generator.js';
import { Id } from './id.js';

describe('UuidIdGenerator', () => {
  it('implements IdGenerator interface', () => {
    const gen: IdGenerator = new UuidIdGenerator();
    const id = gen.newId();
    expect(id).toBeInstanceOf(Id);
  });

  it('newId() returns a valid Id each time', () => {
    const gen = new UuidIdGenerator();
    const id1 = gen.newId();
    const id2 = gen.newId();
    expect(id1).toBeInstanceOf(Id);
    expect(id2).toBeInstanceOf(Id);
    // UUIDs should be non-empty strings
    expect(id1.asStr().length).toBeGreaterThan(0);
    expect(id2.asStr().length).toBeGreaterThan(0);
  });

  it('newId() returns unique ids', () => {
    const gen = new UuidIdGenerator();
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      ids.add(gen.newId().asStr());
    }
    expect(ids.size).toBe(100);
  });
});

describe('FixedIdGenerator', () => {
  it('implements IdGenerator interface', () => {
    const id = Id.fromString('test-id');
    const gen: IdGenerator = new FixedIdGenerator([id]);
    expect(gen.newId()).toBeInstanceOf(Id);
  });

  it('returns ids in FIFO order', () => {
    const id1 = Id.fromString('first');
    const id2 = Id.fromString('second');
    const id3 = Id.fromString('third');
    const gen = new FixedIdGenerator([id1, id2, id3]);

    expect(gen.newId().equals(id1)).toBe(true);
    expect(gen.newId().equals(id2)).toBe(true);
    expect(gen.newId().equals(id3)).toBe(true);
  });

  it('throws when exhausted', () => {
    const gen = new FixedIdGenerator([Id.fromString('only-one')]);
    gen.newId(); // consume the only id
    expect(() => gen.newId()).toThrow('fixed id generator exhausted');
  });

  it('throws immediately when created empty', () => {
    const gen = new FixedIdGenerator([]);
    expect(() => gen.newId()).toThrow('fixed id generator exhausted');
  });

  it('returns exact Id values provided', () => {
    const gen = new FixedIdGenerator([Id.fromString('abc-123'), Id.fromString('def-456')]);
    expect(gen.newId().asStr()).toBe('abc-123');
    expect(gen.newId().asStr()).toBe('def-456');
  });
});
