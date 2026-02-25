import { Id } from './id.js';
import { IdGenerator, UuidIdGenerator } from './id-generator.js';
import { Clock, SystemClock } from '../clock.js';
import { type AssetType } from './asset.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TransactionStatus = 'pending' | 'posted' | 'reversed' | 'canceled' | 'failed';

export interface TransactionStandardizedMetadata {
  merchant_name?: string;
  merchant_category_code?: string;
  merchant_category_label?: string;
  transaction_kind?: string;
  is_internal_transfer_hint?: boolean;
}

export interface TransactionType {
  readonly id: Id;
  readonly timestamp: Date;
  readonly timestamp_raw?: string;
  readonly amount: string;
  readonly asset: AssetType;
  readonly description: string;
  readonly status: TransactionStatus;
  readonly synchronizer_data: unknown;
  readonly standardized_metadata?: TransactionStandardizedMetadata | null;
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
  standardized_metadata?: TransactionStandardizedMetadata;
}

// ---------------------------------------------------------------------------
// Builder functions (create modified copies)
// ---------------------------------------------------------------------------

export function withTimestamp(tx: TransactionType, timestamp: Date): TransactionType {
  return { ...tx, timestamp, timestamp_raw: undefined };
}

export function withStatus(tx: TransactionType, status: TransactionStatus): TransactionType {
  return { ...tx, status };
}

export function withId(tx: TransactionType, id: Id): TransactionType {
  return { ...tx, id };
}

export function withSynchronizerData(tx: TransactionType, data: unknown): TransactionType {
  const derived = deriveStandardizedMetadata(data);
  return {
    ...tx,
    synchronizer_data: data,
    standardized_metadata: mergeStandardizedMetadata(tx.standardized_metadata ?? null, derived),
  };
}

export function withStandardizedMetadata(
  tx: TransactionType,
  metadata: TransactionStandardizedMetadata | null,
): TransactionType {
  return { ...tx, standardized_metadata: isEmptyMetadata(metadata) ? null : metadata };
}

function nonEmpty(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function firstNonEmptyArrayString(value: unknown): string | undefined {
  if (!Array.isArray(value)) return undefined;
  for (const item of value) {
    const s = nonEmpty(item);
    if (s !== undefined) return s;
  }
  return undefined;
}

function normalizeCategoryLabel(raw: string | undefined): string | undefined {
  if (raw === undefined) return undefined;
  const normalized = raw.trim().replace(/[_-]+/g, ' ');
  if (normalized === '') return undefined;
  const words = normalized
    .split(/\s+/)
    .map((word) => {
      if (word.length === 0) return '';
      return `${word[0]!.toUpperCase()}${word.slice(1).toLowerCase()}`;
    })
    .filter((word) => word.length > 0);
  return words.length > 0 ? words.join(' ') : undefined;
}

function normalizeTransactionKind(raw: string | undefined): string | undefined {
  if (raw === undefined) return undefined;
  const value = raw.trim().toLowerCase();
  if (value === '') return undefined;
  if (value.includes('purchase')) return 'purchase';
  if (value.includes('payment')) return 'payment';
  if (value.includes('transfer')) return 'transfer';
  if (value.includes('fee')) return 'fee';
  if (value.includes('interest')) return 'interest';
  if (value.includes('refund')) return 'refund';
  if (value.includes('deposit')) return 'deposit';
  if (value.includes('withdraw')) return 'withdrawal';
  return undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isEmptyMetadata(value: TransactionStandardizedMetadata | null | undefined): boolean {
  if (value === null || value === undefined) return true;
  return Object.keys(value).length === 0;
}

function deriveStandardizedMetadata(
  synchronizerData: unknown,
): TransactionStandardizedMetadata | null {
  if (!isRecord(synchronizerData)) return null;

  const merchant_name =
    firstNonEmptyArrayString(synchronizerData.enriched_merchant_names) ??
    nonEmpty(synchronizerData.merchant_dba_name) ??
    nonEmpty(synchronizerData.merchant_name);
  const merchant_category_code = nonEmpty(synchronizerData.merchant_category_code);
  const merchant_category_label =
    nonEmpty(synchronizerData.merchant_category_name) ??
    normalizeCategoryLabel(nonEmpty(synchronizerData.etu_standard_expense_category_code));
  const transaction_kind = normalizeTransactionKind(
    nonEmpty(synchronizerData.etu_standard_transaction_type_group_name) ??
      nonEmpty(synchronizerData.etu_standard_transaction_type_name),
  );
  const is_internal_transfer_hint =
    transaction_kind === 'transfer' || transaction_kind === 'payment'
      ? true
      : transaction_kind === undefined
        ? undefined
        : false;

  const metadata: TransactionStandardizedMetadata = {
    ...(merchant_name !== undefined ? { merchant_name } : {}),
    ...(merchant_category_code !== undefined ? { merchant_category_code } : {}),
    ...(merchant_category_label !== undefined ? { merchant_category_label } : {}),
    ...(transaction_kind !== undefined ? { transaction_kind } : {}),
    ...(is_internal_transfer_hint !== undefined ? { is_internal_transfer_hint } : {}),
  };

  return Object.keys(metadata).length > 0 ? metadata : null;
}

function mergeStandardizedMetadata(
  existing: TransactionStandardizedMetadata | null | undefined,
  derived: TransactionStandardizedMetadata | null,
): TransactionStandardizedMetadata | null {
  const left = isEmptyMetadata(existing) ? null : existing ?? null;
  if (left === null) return derived;
  if (derived === null) return left;

  const merchant_name = left.merchant_name ?? derived.merchant_name;
  const merchant_category_code = left.merchant_category_code ?? derived.merchant_category_code;
  const merchant_category_label = left.merchant_category_label ?? derived.merchant_category_label;
  const transaction_kind = left.transaction_kind ?? derived.transaction_kind;
  const is_internal_transfer_hint =
    left.is_internal_transfer_hint ?? derived.is_internal_transfer_hint;

  const merged: TransactionStandardizedMetadata = {
    ...(merchant_name !== undefined ? { merchant_name } : {}),
    ...(merchant_category_code !== undefined ? { merchant_category_code } : {}),
    ...(merchant_category_label !== undefined ? { merchant_category_label } : {}),
    ...(transaction_kind !== undefined ? { transaction_kind } : {}),
    ...(is_internal_transfer_hint !== undefined ? { is_internal_transfer_hint } : {}),
  };
  return Object.keys(merged).length > 0 ? merged : null;
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
      standardized_metadata: null,
    };
  },

  /**
   * Serialize a transaction to a plain JSON-serializable object.
   * Omits synchronizer_data when null.
   */
  toJSON(tx: TransactionType): TransactionJSON {
    const json: TransactionJSON = {
      id: tx.id.toJSON(),
      timestamp: tx.timestamp_raw ?? tx.timestamp.toISOString(),
      amount: tx.amount,
      asset: tx.asset,
      description: tx.description,
      status: tx.status,
    };
    if (tx.synchronizer_data !== null) {
      json.synchronizer_data = tx.synchronizer_data;
    }
    if (!isEmptyMetadata(tx.standardized_metadata)) {
      json.standardized_metadata = tx.standardized_metadata ?? undefined;
    }
    return json;
  },

  /**
   * Deserialize a transaction from a JSON object.
   */
  fromJSON(json: TransactionJSON): TransactionType {
    const synchronizerData = json.synchronizer_data ?? null;
    const standardizedMetadata = mergeStandardizedMetadata(
      json.standardized_metadata ?? null,
      deriveStandardizedMetadata(synchronizerData),
    );

    return {
      id: Id.fromString(json.id),
      timestamp: new Date(json.timestamp),
      timestamp_raw: json.timestamp,
      amount: json.amount,
      asset: json.asset,
      description: json.description,
      status: json.status,
      synchronizer_data: synchronizerData,
      standardized_metadata: standardizedMetadata,
    };
  },
} as const;
