import { Id } from './id.js';

/**
 * Abstraction over ID generation to support deterministic tests.
 */
export interface IdGenerator {
  newId(): Id;
}

/**
 * Generates random UUID v4 IDs.
 */
export class UuidIdGenerator implements IdGenerator {
  newId(): Id {
    return Id.new();
  }
}

/**
 * A deterministic generator that returns a pre-seeded sequence of IDs.
 *
 * Throws if you request more IDs than provided.
 */
export class FixedIdGenerator implements IdGenerator {
  private readonly ids: Id[];

  constructor(ids: Iterable<Id>) {
    this.ids = Array.from(ids);
  }

  newId(): Id {
    const id = this.ids.shift();
    if (id === undefined) {
      throw new Error('fixed id generator exhausted');
    }
    return id;
  }
}
