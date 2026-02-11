/**
 * keepbook — TypeScript port of the Rust keepbook library.
 *
 * Re-exports all public API surface from a single entry point.
 */

// ---------------------------------------------------------------------------
// Core utilities
// ---------------------------------------------------------------------------

export { Clock, SystemClock, FixedClock } from './clock.js';
export { parseDuration, formatDuration } from './duration.js';
export {
  Config,
  RefreshConfig,
  GitConfig,
  parseConfig,
  resolveDataDir,
  DEFAULT_CONFIG,
  DEFAULT_REFRESH_CONFIG,
  DEFAULT_GIT_CONFIG,
} from './config.js';
export {
  StalenessCheck,
  resolveBalanceStaleness,
  checkBalanceStalenessAt,
  checkPriceStalenessAt,
} from './staleness.js';

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

export { Id, IdError } from './models/id.js';
export { type IdGenerator, UuidIdGenerator, FixedIdGenerator } from './models/id-generator.js';
export {
  Asset,
  type AssetType,
  type CurrencyAsset,
  type EquityAsset,
  type CryptoAsset,
} from './models/asset.js';
export {
  Account,
  type AccountType,
  type AccountConfig,
  type BalanceBackfillPolicy,
} from './models/account.js';
export {
  Transaction,
  type TransactionType,
  type TransactionStatus,
  type TransactionJSON,
  withTimestamp,
  withStatus,
  withId,
  withSynchronizerData,
} from './models/transaction.js';
export {
  AssetBalance,
  BalanceSnapshot,
  type AssetBalanceType,
  type BalanceSnapshotType,
  type AssetBalanceJSON,
  type BalanceSnapshotJSON,
} from './models/balance.js';
export {
  Connection,
  ConnectionState,
  type ConnectionType,
  type ConnectionStateType,
  type ConnectionConfig,
  type ConnectionStatus,
  type SyncStatus,
  type LastSync,
  type CredentialConfig,
  type ConnectionJSON,
  type ConnectionStateJSON,
  type ConnectionConfigJSON,
} from './models/connection.js';

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

export { type Storage, type CredentialStore } from './storage/storage.js';
export { MemoryStorage } from './storage/memory.js';
export { JsonFileStorage } from './storage/json-file.js';
export { findConnection, findAccount } from './storage/lookup.js';

// ---------------------------------------------------------------------------
// Market data
// ---------------------------------------------------------------------------

export { AssetId } from './market-data/asset-id.js';
export {
  sanitizeSegment,
  normalizeUpperSegment,
  normalizeLowerSegment,
} from './market-data/asset-id.js';
export {
  type PriceKind,
  type FxRateKind,
  type PricePoint,
  type FxRatePoint,
  type AssetRegistryEntry,
  AssetRegistryEntryFactory,
  pricePointToJSON,
  pricePointFromJSON,
  fxRatePointToJSON,
  fxRatePointFromJSON,
  assetRegistryEntryToJSON,
  assetRegistryEntryFromJSON,
  type PricePointJSON,
  type FxRatePointJSON,
  type AssetRegistryEntryJSON,
} from './market-data/models.js';
export {
  type MarketDataStore,
  NullMarketDataStore,
  MemoryMarketDataStore,
} from './market-data/store.js';
export { JsonlMarketDataStore } from './market-data/jsonl-store.js';
export {
  type MarketDataSource,
  type EquityPriceSource,
  type CryptoPriceSource,
  type FxRateSource,
  EquityPriceRouter,
  CryptoPriceRouter,
  FxRateRouter,
} from './market-data/sources.js';
export { MarketDataService } from './market-data/service.js';

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

export {
  type PassConfig,
  type CredentialConfig as CredentialConfigUnion,
  parseCredentialConfig,
} from './credentials/index.js';

// ---------------------------------------------------------------------------
// Portfolio
// ---------------------------------------------------------------------------

export {
  type Grouping,
  type PortfolioQuery,
  type PortfolioSnapshot,
  type AssetSummary,
  type AccountHolding,
  type AccountSummary,
} from './portfolio/models.js';
export { PortfolioService } from './portfolio/service.js';
export {
  type ChangePoint,
  type ChangeTrigger,
  type Granularity,
  type CoalesceStrategy,
  ChangePointCollector,
  filterByGranularity,
  filterByDateRange,
  collectChangePoints,
} from './portfolio/change-points.js';

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

export {
  type AuthStatus,
  type SyncedAssetBalance,
  SyncedAssetBalanceFactory,
  type SyncResult,
  saveSyncResult,
  type Synchronizer,
  type InteractiveAuth,
} from './sync/mod.js';

// ---------------------------------------------------------------------------
// Git
// ---------------------------------------------------------------------------

export { type AutoCommitOutcome, tryAutoCommit } from './git.js';
