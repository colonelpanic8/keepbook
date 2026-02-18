import { Id } from './id.js';
import { IdGenerator, UuidIdGenerator } from './id-generator.js';
import { Clock, SystemClock } from '../clock.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type BalanceBackfillPolicy = 'none' | 'zero' | 'carry_earliest';

export interface AccountType {
  readonly id: Id;
  readonly name: string;
  readonly connection_id: Id;
  readonly tags: string[];
  readonly created_at: Date;
  readonly active: boolean;
  readonly synchronizer_data: unknown;
}

export interface AccountConfig {
  readonly balance_staleness?: number;
  readonly balance_backfill?: BalanceBackfillPolicy;
  readonly exclude_from_portfolio?: boolean;
}

// ---------------------------------------------------------------------------
// JSON types
// ---------------------------------------------------------------------------

export interface AccountJSON {
  id: string;
  name: string;
  connection_id: string;
  tags: string[];
  created_at: string;
  active: boolean;
  synchronizer_data?: unknown;
}

// ---------------------------------------------------------------------------
// Account namespace (factory functions + serialization)
// ---------------------------------------------------------------------------

export const Account = {
  /**
   * Create a new account with auto-generated id and current time.
   */
  new(name: string, connectionId: Id): AccountType {
    return Account.newWithGenerator(new UuidIdGenerator(), new SystemClock(), name, connectionId);
  },

  /**
   * Create an account with explicit id and timestamp.
   */
  newWith(id: Id, createdAt: Date, name: string, connectionId: Id): AccountType {
    return {
      id,
      name,
      connection_id: connectionId,
      tags: [],
      created_at: createdAt,
      active: true,
      synchronizer_data: null,
    };
  },

  /**
   * Create an account using injectable id generator and clock.
   */
  newWithGenerator(ids: IdGenerator, clock: Clock, name: string, connectionId: Id): AccountType {
    return Account.newWith(ids.newId(), clock.now(), name, connectionId);
  },

  /**
   * Serialize an account to a plain JSON-serializable object.
   * Uses snake_case field names. Omits synchronizer_data when null.
   */
  toJSON(account: AccountType): AccountJSON {
    const json: AccountJSON = {
      id: account.id.toJSON(),
      name: account.name,
      connection_id: account.connection_id.toJSON(),
      tags: [...account.tags],
      created_at: account.created_at.toISOString(),
      active: account.active,
    };
    if (account.synchronizer_data !== null) {
      json.synchronizer_data = account.synchronizer_data;
    }
    return json;
  },

  /**
   * Deserialize an account from a JSON object.
   */
  fromJSON(json: AccountJSON): AccountType {
    return {
      id: Id.fromString(json.id),
      name: json.name,
      connection_id: Id.fromString(json.connection_id),
      tags: [...json.tags],
      created_at: new Date(json.created_at),
      active: json.active,
      synchronizer_data: json.synchronizer_data ?? null,
    };
  },
} as const;
