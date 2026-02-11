import { type AssetType } from './asset.js';
import { Clock, SystemClock } from '../clock.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AssetBalanceType {
  readonly asset: AssetType;
  readonly amount: string;
}

export interface BalanceSnapshotType {
  readonly timestamp: Date;
  readonly timestamp_raw?: string;
  readonly balances: AssetBalanceType[];
}

// ---------------------------------------------------------------------------
// JSON types
// ---------------------------------------------------------------------------

export interface AssetBalanceJSON {
  asset: AssetType;
  amount: string;
}

export interface BalanceSnapshotJSON {
  timestamp: string;
  balances: AssetBalanceJSON[];
}

// ---------------------------------------------------------------------------
// AssetBalance namespace
// ---------------------------------------------------------------------------

export const AssetBalance = {
  /**
   * Create an asset balance.
   */
  new(asset: AssetType, amount: string): AssetBalanceType {
    return { asset, amount };
  },

  /**
   * Serialize to JSON.
   */
  toJSON(balance: AssetBalanceType): AssetBalanceJSON {
    return {
      asset: balance.asset,
      amount: balance.amount,
    };
  },

  /**
   * Deserialize from JSON.
   */
  fromJSON(json: AssetBalanceJSON): AssetBalanceType {
    return {
      asset: json.asset,
      amount: json.amount,
    };
  },
} as const;

// ---------------------------------------------------------------------------
// BalanceSnapshot namespace
// ---------------------------------------------------------------------------

export const BalanceSnapshot = {
  /**
   * Create a balance snapshot with a given timestamp and balances.
   */
  new(timestamp: Date, balances: AssetBalanceType[]): BalanceSnapshotType {
    return { timestamp, balances: [...balances] };
  },

  /**
   * Create a balance snapshot at the current time.
   */
  now(balances: AssetBalanceType[]): BalanceSnapshotType {
    return BalanceSnapshot.nowWith(new SystemClock(), balances);
  },

  /**
   * Create a balance snapshot using an injected clock.
   */
  nowWith(clock: Clock, balances: AssetBalanceType[]): BalanceSnapshotType {
    return BalanceSnapshot.new(clock.now(), balances);
  },

  /**
   * Serialize to JSON.
   */
  toJSON(snapshot: BalanceSnapshotType): BalanceSnapshotJSON {
    return {
      timestamp: snapshot.timestamp_raw ?? snapshot.timestamp.toISOString(),
      balances: snapshot.balances.map(AssetBalance.toJSON),
    };
  },

  /**
   * Deserialize from JSON.
   */
  fromJSON(json: BalanceSnapshotJSON): BalanceSnapshotType {
    return {
      timestamp: new Date(json.timestamp),
      timestamp_raw: json.timestamp,
      balances: json.balances.map(AssetBalance.fromJSON),
    };
  },
} as const;
