import { Id } from './id.js';
import { IdGenerator, UuidIdGenerator } from './id-generator.js';
import { Clock, SystemClock } from '../clock.js';
import { type AssetType } from './asset.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TransactionStatus = 'pending' | 'posted' | 'reversed' | 'canceled' | 'failed';

export interface TransactionType {
  readonly id: Id;
  readonly timestamp: Date;
  readonly amount: string;
  readonly asset: AssetType;
  readonly description: string;
  readonly status: TransactionStatus;
  readonly synchronizer_data: unknown;
}

// ---------------------------------------------------------------------------
// JSON types
// ---------------------------------------------------------------------------

export interface TransactionJSON {
  id: string;
  timestamp: string;
  amount: string;
  asset: AssetType;
  description: string;
  status: TransactionStatus;
  synchronizer_data?: unknown;
}

// ---------------------------------------------------------------------------
// Builder functions (create modified copies)
// ---------------------------------------------------------------------------

export function withTimestamp(tx: TransactionType, timestamp: Date): TransactionType {
  return { ...tx, timestamp };
}

export function withStatus(tx: TransactionType, status: TransactionStatus): TransactionType {
  return { ...tx, status };
}

export function withId(tx: TransactionType, id: Id): TransactionType {
  return { ...tx, id };
}

export function withSynchronizerData(tx: TransactionType, data: unknown): TransactionType {
  return { ...tx, synchronizer_data: data };
}

// ---------------------------------------------------------------------------
// Transaction namespace (factory functions + serialization)
// ---------------------------------------------------------------------------

export const Transaction = {
  /**
   * Create a new transaction with auto-generated id and current time.
   * Defaults to 'posted' status.
   */
  new(amount: string, asset: AssetType, description: string): TransactionType {
    return Transaction.newWithGenerator(
      new UuidIdGenerator(),
      new SystemClock(),
      amount,
      asset,
      description,
    );
  },

  /**
   * Create a transaction using injectable id generator and clock.
   * Defaults to 'posted' status.
   */
  newWithGenerator(
    ids: IdGenerator,
    clock: Clock,
    amount: string,
    asset: AssetType,
    description: string,
  ): TransactionType {
    return {
      id: ids.newId(),
      timestamp: clock.now(),
      amount,
      asset,
      description,
      status: 'posted',
      synchronizer_data: null,
    };
  },

  /**
   * Serialize a transaction to a plain JSON-serializable object.
   * Omits synchronizer_data when null.
   */
  toJSON(tx: TransactionType): TransactionJSON {
    const json: TransactionJSON = {
      id: tx.id.toJSON(),
      timestamp: tx.timestamp.toISOString(),
      amount: tx.amount,
      asset: tx.asset,
      description: tx.description,
      status: tx.status,
    };
    if (tx.synchronizer_data !== null) {
      json.synchronizer_data = tx.synchronizer_data;
    }
    return json;
  },

  /**
   * Deserialize a transaction from a JSON object.
   */
  fromJSON(json: TransactionJSON): TransactionType {
    return {
      id: Id.fromString(json.id),
      timestamp: new Date(json.timestamp),
      amount: json.amount,
      asset: json.asset,
      description: json.description,
      status: json.status,
      synchronizer_data: json.synchronizer_data ?? null,
    };
  },
} as const;
