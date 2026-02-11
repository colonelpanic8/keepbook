# TypeScript Port of Keepbook Rust Library

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a feature-equivalent TypeScript implementation of the keepbook Rust library in the `ts/` directory, built TDD-style.

**Architecture:** Mirror the Rust module structure using TypeScript interfaces for traits and classes for structs. Use dependency injection (interfaces) for storage, clock, market data, and credentials to maintain testability. All amounts stored as strings, computed via `Decimal.js` for precision.

**Tech Stack:** TypeScript 5, Vitest for testing, `uuid` for IDs, `decimal.js` for arithmetic, `date-fns` for date utilities, `toml` for config parsing, Node.js `fs/promises` for file storage.

---

## Project Setup

### Task 1: Initialize TypeScript project

**Files:**
- Create: `ts/package.json`
- Create: `ts/tsconfig.json`
- Create: `ts/vitest.config.ts`
- Create: `ts/.gitignore`

**Step 1: Create project directory and package.json**

```bash
mkdir -p ts
```

```json
// ts/package.json
{
  "name": "keepbook",
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest",
    "build": "tsc",
    "typecheck": "tsc --noEmit"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "vitest": "^3.0.0",
    "@types/node": "^22.0.0",
    "@types/uuid": "^10.0.0"
  },
  "dependencies": {
    "uuid": "^11.0.0",
    "decimal.js": "^10.4.0",
    "date-fns": "^4.0.0",
    "toml": "^3.0.0"
  }
}
```

**Step 2: Create tsconfig.json**

```json
// ts/tsconfig.json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "Node16",
    "moduleResolution": "Node16",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "rootDir": "src",
    "declaration": true,
    "sourceMap": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src"],
  "exclude": ["node_modules", "dist"]
}
```

**Step 3: Create vitest.config.ts**

```typescript
// ts/vitest.config.ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    include: ['src/**/*.test.ts'],
  },
});
```

**Step 4: Create .gitignore**

```
node_modules/
dist/
```

**Step 5: Install dependencies**

Run: `cd ts && npm install`

**Step 6: Commit**

```bash
git add ts/package.json ts/tsconfig.json ts/vitest.config.ts ts/.gitignore ts/package-lock.json
git commit -m "feat(ts): initialize TypeScript project with vitest"
```

---

## Core Modules

### Task 2: Clock module

**Files:**
- Test: `ts/src/clock.test.ts`
- Create: `ts/src/clock.ts`

**Step 1: Write the failing test**

```typescript
// ts/src/clock.test.ts
import { describe, it, expect } from 'vitest';
import { SystemClock, FixedClock } from './clock.js';

describe('SystemClock', () => {
  it('returns current time', () => {
    const clock = new SystemClock();
    const before = new Date();
    const now = clock.now();
    const after = new Date();
    expect(now.getTime()).toBeGreaterThanOrEqual(before.getTime());
    expect(now.getTime()).toBeLessThanOrEqual(after.getTime());
  });

  it('today returns date without time', () => {
    const clock = new SystemClock();
    const today = clock.today();
    // today is a YYYY-MM-DD string
    expect(today).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe('FixedClock', () => {
  it('returns fixed time', () => {
    const fixed = new Date('2026-02-05T12:00:00Z');
    const clock = new FixedClock(fixed);
    expect(clock.now()).toEqual(fixed);
    expect(clock.now()).toEqual(fixed);
  });

  it('today returns date portion', () => {
    const fixed = new Date('2026-02-05T12:00:00Z');
    const clock = new FixedClock(fixed);
    expect(clock.today()).toBe('2026-02-05');
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd ts && npx vitest run src/clock.test.ts`
Expected: FAIL (module not found)

**Step 3: Write minimal implementation**

```typescript
// ts/src/clock.ts

/** Abstraction over current time for deterministic testing. */
export interface Clock {
  now(): Date;
  today(): string; // YYYY-MM-DD (NaiveDate equivalent)
}

export class SystemClock implements Clock {
  now(): Date {
    return new Date();
  }

  today(): string {
    return this.now().toISOString().slice(0, 10);
  }
}

export class FixedClock implements Clock {
  constructor(private readonly _now: Date) {}

  now(): Date {
    return this._now;
  }

  today(): string {
    return this._now.toISOString().slice(0, 10);
  }
}
```

**Step 4: Run test to verify it passes**

Run: `cd ts && npx vitest run src/clock.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add ts/src/clock.ts ts/src/clock.test.ts
git commit -m "feat(ts): add Clock interface with SystemClock and FixedClock"
```

---

### Task 3: Duration parsing module

**Files:**
- Test: `ts/src/duration.test.ts`
- Create: `ts/src/duration.ts`

Port `src/duration.rs` — parse "14d", "24h", "30m", "60s" to milliseconds, format back. Case-insensitive, whitespace-trimmed.

**Step 1: Write the failing test**

```typescript
// ts/src/duration.test.ts
import { describe, it, expect } from 'vitest';
import { parseDuration, formatDuration } from './duration.js';

describe('parseDuration', () => {
  it('parses days', () => {
    expect(parseDuration('1d')).toBe(86400_000);
    expect(parseDuration('14d')).toBe(14 * 86400_000);
  });

  it('parses hours', () => {
    expect(parseDuration('1h')).toBe(3600_000);
    expect(parseDuration('24h')).toBe(24 * 3600_000);
  });

  it('parses minutes', () => {
    expect(parseDuration('1m')).toBe(60_000);
    expect(parseDuration('30m')).toBe(30 * 60_000);
  });

  it('parses seconds', () => {
    expect(parseDuration('1s')).toBe(1_000);
    expect(parseDuration('60s')).toBe(60_000);
  });

  it('is case insensitive', () => {
    expect(parseDuration('1D')).toBe(86400_000);
    expect(parseDuration('1H')).toBe(3600_000);
    expect(parseDuration('1M')).toBe(60_000);
    expect(parseDuration('1S')).toBe(1_000);
  });

  it('trims whitespace', () => {
    expect(parseDuration('  1d  ')).toBe(86400_000);
    expect(parseDuration('\t24h\n')).toBe(24 * 3600_000);
  });

  it('rejects invalid unit', () => {
    expect(() => parseDuration('1x')).toThrow();
    expect(() => parseDuration('1w')).toThrow();
    expect(() => parseDuration('1')).toThrow();
    expect(() => parseDuration('d')).toThrow();
  });

  it('rejects invalid number', () => {
    expect(() => parseDuration('abcd')).toThrow();
    expect(() => parseDuration('-1d')).toThrow();
    expect(() => parseDuration('1.5h')).toThrow();
  });

  it('rejects empty input', () => {
    expect(() => parseDuration('')).toThrow();
    expect(() => parseDuration('   ')).toThrow();
  });
});

describe('formatDuration', () => {
  it('formats days', () => {
    expect(formatDuration(86400_000)).toBe('1d');
    expect(formatDuration(14 * 86400_000)).toBe('14d');
  });

  it('formats hours', () => {
    expect(formatDuration(3600_000)).toBe('1h');
    expect(formatDuration(12 * 3600_000)).toBe('12h');
  });

  it('formats minutes', () => {
    expect(formatDuration(60_000)).toBe('1m');
    expect(formatDuration(30 * 60_000)).toBe('30m');
  });

  it('formats seconds', () => {
    expect(formatDuration(1_000)).toBe('1s');
    expect(formatDuration(45_000)).toBe('45s');
  });

  it('formats zero', () => {
    expect(formatDuration(0)).toBe('0s');
  });

  it('formats non-divisible as seconds', () => {
    expect(formatDuration(90_000)).toBe('90s');
    expect(formatDuration(3700_000)).toBe('3700s');
  });

  it('roundtrips', () => {
    const values = [86400_000, 14 * 86400_000, 3600_000, 24 * 3600_000, 60_000, 30 * 60_000, 1_000, 45_000];
    for (const v of values) {
      expect(parseDuration(formatDuration(v))).toBe(v);
    }
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd ts && npx vitest run src/duration.test.ts`

**Step 3: Write minimal implementation**

```typescript
// ts/src/duration.ts

const SECS_PER_DAY = 24 * 60 * 60;
const SECS_PER_HOUR = 60 * 60;
const SECS_PER_MINUTE = 60;

/** Parse a human duration string ("14d", "24h", "30m", "60s") into milliseconds. */
export function parseDuration(s: string): number {
  const trimmed = s.trim().toLowerCase();
  if (trimmed.length === 0) throw new Error('Duration must not be empty');

  const unit = trimmed[trimmed.length - 1];
  if (!['d', 'h', 'm', 's'].includes(unit)) {
    throw new Error('Duration must end with d, h, m, or s');
  }

  const numStr = trimmed.slice(0, -1);
  const num = Number(numStr);
  if (!Number.isInteger(num) || num < 0 || numStr === '') {
    throw new Error(`Invalid number in duration: ${numStr}`);
  }

  let secs: number;
  switch (unit) {
    case 'd': secs = num * SECS_PER_DAY; break;
    case 'h': secs = num * SECS_PER_HOUR; break;
    case 'm': secs = num * SECS_PER_MINUTE; break;
    case 's': secs = num; break;
    default: throw new Error('unreachable');
  }

  return secs * 1000;
}

/** Format milliseconds to a human-readable duration string. */
export function formatDuration(ms: number): string {
  const secs = Math.floor(ms / 1000);

  if (secs >= SECS_PER_DAY && secs % SECS_PER_DAY === 0) {
    return `${secs / SECS_PER_DAY}d`;
  }
  if (secs >= SECS_PER_HOUR && secs % SECS_PER_HOUR === 0) {
    return `${secs / SECS_PER_HOUR}h`;
  }
  if (secs >= SECS_PER_MINUTE && secs % SECS_PER_MINUTE === 0) {
    return `${secs / SECS_PER_MINUTE}m`;
  }
  return `${secs}s`;
}
```

**Step 4: Run test to verify it passes**

Run: `cd ts && npx vitest run src/duration.test.ts`

**Step 5: Commit**

```bash
git add ts/src/duration.ts ts/src/duration.test.ts
git commit -m "feat(ts): add duration parsing and formatting"
```

---

### Task 4: Id module

**Files:**
- Test: `ts/src/models/id.test.ts`
- Create: `ts/src/models/id.ts`

Port `src/models/id.rs` — UUID v4, UUID v5 from external, path safety checks.

**Step 1: Write the failing test**

```typescript
// ts/src/models/id.test.ts
import { describe, it, expect } from 'vitest';
import { Id } from './id.js';

describe('Id', () => {
  it('new() generates unique ids', () => {
    const a = Id.new();
    const b = Id.new();
    expect(a.asStr()).not.toBe(b.asStr());
  });

  it('fromString keeps value', () => {
    const id = Id.fromString('account-id-123');
    expect(id.asStr()).toBe('account-id-123');
  });

  it('fromExternal is deterministic', () => {
    const a = Id.fromExternal('schwab-account-123');
    const b = Id.fromExternal('schwab-account-123');
    expect(a.asStr()).toBe(b.asStr());
  });

  it('fromExternal differs for different inputs', () => {
    const a = Id.fromExternal('schwab-account-123');
    const b = Id.fromExternal('schwab-account-456');
    expect(a.asStr()).not.toBe(b.asStr());
  });

  it('fromExternal is path safe', () => {
    const id = Id.fromExternal('weird/account/value');
    expect(id.asStr()).not.toContain('/');
  });

  it('fromStringChecked rejects unsafe values', () => {
    expect(() => Id.fromStringChecked('../escape')).toThrow();
    expect(() => Id.fromStringChecked('..')).toThrow();
    expect(() => Id.fromStringChecked('.')).toThrow();
    expect(() => Id.fromStringChecked('foo/bar')).toThrow();
    expect(() => Id.fromStringChecked('foo\\bar')).toThrow();
    expect(() => Id.fromStringChecked('bad\0id')).toThrow();
  });

  it('fromStringChecked accepts safe values', () => {
    const id = Id.fromStringChecked('good-id-123');
    expect(id.asStr()).toBe('good-id-123');
  });

  it('isPathSafe validates correctly', () => {
    expect(Id.isPathSafe('')).toBe(false);
    expect(Id.isPathSafe('.')).toBe(false);
    expect(Id.isPathSafe('..')).toBe(false);
    expect(Id.isPathSafe('foo/bar')).toBe(false);
    expect(Id.isPathSafe('foo\\bar')).toBe(false);
    expect(Id.isPathSafe('valid-id')).toBe(true);
  });

  it('equality works', () => {
    const a = Id.fromString('same');
    const b = Id.fromString('same');
    expect(a.equals(b)).toBe(true);
    expect(a.equals(Id.fromString('different'))).toBe(false);
  });

  it('toString returns the id string', () => {
    const id = Id.fromString('my-id');
    expect(id.toString()).toBe('my-id');
  });

  it('serializes as plain string in JSON', () => {
    const id = Id.fromString('my-id');
    expect(JSON.stringify(id)).toBe('"my-id"');
  });
});
```

**Step 2: Run test to verify it fails**

**Step 3: Write minimal implementation**

```typescript
// ts/src/models/id.ts
import { v4 as uuidv4, v5 as uuidv5 } from 'uuid';

const NAMESPACE = '6ba7b810-9dad-11d1-80b4-00c04fd430c8';

export class Id {
  private constructor(private readonly value: string) {}

  static new(): Id {
    return new Id(uuidv4());
  }

  static fromString(value: string): Id {
    return new Id(value);
  }

  static fromStringChecked(value: string): Id {
    if (!Id.isPathSafe(value)) {
      throw new Error(
        `Invalid id "${value}": ids must be a single path segment (no '/', '\\', NUL, '.' or '..')`
      );
    }
    return new Id(value);
  }

  static fromExternal(value: string): Id {
    return new Id(uuidv5(value, NAMESPACE));
  }

  static isPathSafe(value: string): boolean {
    if (value === '' || value === '.' || value === '..') return false;
    return ![...value].some(c => c === '/' || c === '\\' || c === '\0');
  }

  asStr(): string {
    return this.value;
  }

  toString(): string {
    return this.value;
  }

  equals(other: Id): boolean {
    return this.value === other.value;
  }

  toJSON(): string {
    return this.value;
  }
}
```

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add ts/src/models/id.ts ts/src/models/id.test.ts
git commit -m "feat(ts): add Id with UUID v4/v5 and path safety"
```

---

### Task 5: IdGenerator module

**Files:**
- Test: `ts/src/models/id-generator.test.ts`
- Create: `ts/src/models/id-generator.ts`

Port `src/models/id_generator.rs`.

**Step 1: Write the failing test**

```typescript
// ts/src/models/id-generator.test.ts
import { describe, it, expect } from 'vitest';
import { UuidIdGenerator, FixedIdGenerator } from './id-generator.js';
import { Id } from './id.js';

describe('UuidIdGenerator', () => {
  it('generates unique ids', () => {
    const gen = new UuidIdGenerator();
    const a = gen.newId();
    const b = gen.newId();
    expect(a.asStr()).not.toBe(b.asStr());
  });
});

describe('FixedIdGenerator', () => {
  it('returns pre-seeded ids in order', () => {
    const ids = [Id.fromString('id-1'), Id.fromString('id-2'), Id.fromString('id-3')];
    const gen = new FixedIdGenerator(ids);
    expect(gen.newId().asStr()).toBe('id-1');
    expect(gen.newId().asStr()).toBe('id-2');
    expect(gen.newId().asStr()).toBe('id-3');
  });

  it('throws when exhausted', () => {
    const gen = new FixedIdGenerator([Id.fromString('only-one')]);
    gen.newId();
    expect(() => gen.newId()).toThrow('exhausted');
  });
});
```

**Step 2: Run test to verify it fails**

**Step 3: Write minimal implementation**

```typescript
// ts/src/models/id-generator.ts
import { Id } from './id.js';

export interface IdGenerator {
  newId(): Id;
}

export class UuidIdGenerator implements IdGenerator {
  newId(): Id {
    return Id.new();
  }
}

export class FixedIdGenerator implements IdGenerator {
  private queue: Id[];
  private index = 0;

  constructor(ids: Id[]) {
    this.queue = [...ids];
  }

  newId(): Id {
    if (this.index >= this.queue.length) {
      throw new Error('fixed id generator exhausted');
    }
    return this.queue[this.index++];
  }
}
```

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add ts/src/models/id-generator.ts ts/src/models/id-generator.test.ts
git commit -m "feat(ts): add IdGenerator with UUID and Fixed implementations"
```

---

### Task 6: Asset module

**Files:**
- Test: `ts/src/models/asset.test.ts`
- Create: `ts/src/models/asset.ts`

Port `src/models/asset.rs` — discriminated union with case-insensitive equality and normalization. Serializes with `{ "type": "currency", "iso_code": "USD" }`.

**Step 1: Write the failing tests** — cover serialization, case-insensitive equality, normalization, and factory methods. Match the Rust tests exactly.

**Step 2: Run test to verify it fails**

**Step 3: Write implementation** — Use discriminated union type `{ type: 'currency'; isoCode: string } | { type: 'equity'; ticker: string; exchange?: string } | { type: 'crypto'; symbol: string; network?: string }`. Implement `assetEquals()`, `assetHash()`, `normalizeAsset()`, and `serializeAsset()`/`deserializeAsset()` that match Rust's serde format (snake_case field names like `iso_code`).

**Step 4: Run tests, verify pass**

**Step 5: Commit**

---

### Task 7: Core model types (Account, Transaction, BalanceSnapshot, Connection)

**Files:**
- Test: `ts/src/models/models.test.ts`
- Create: `ts/src/models/account.ts`
- Create: `ts/src/models/transaction.ts`
- Create: `ts/src/models/balance.ts`
- Create: `ts/src/models/connection.ts`
- Create: `ts/src/models/index.ts` (barrel export)

Port these Rust model files:
- `src/models/account.rs` → `Account`, `AccountConfig`, `BalanceBackfillPolicy`
- `src/models/transaction.rs` → `Transaction`, `TransactionStatus`
- `src/models/balance.rs` → `AssetBalance`, `BalanceSnapshot`
- `src/models/connection.rs` → `Connection`, `ConnectionConfig`, `ConnectionState`, `ConnectionStatus`, `LastSync`, `SyncStatus`

All types use JSON-serializable field names matching Rust's serde `snake_case` format. `synchronizer_data` is `unknown` (JSON). `with_*` builder methods as fluent API on Transaction. Factory methods `new()` and `newWithGenerator()`.

**Step 1: Write tests** for construction, builder, deterministic generators, JSON serialization

**Step 2-5: Implement, run, commit**

---

### Task 8: AssetId module

**Files:**
- Test: `ts/src/market-data/asset-id.test.ts`
- Create: `ts/src/market-data/asset-id.ts`

Port `src/market_data/asset_id.rs` — deterministic path-safe asset identifiers. `currency/USD`, `equity/AAPL`, `equity/AAPL/NYSE`, `crypto/BTC`, `crypto/ETH/arbitrum`. Sanitizes `/`, `\`, NUL to `-`. Normalizes case.

**Step 1: Write tests** — cover all the Rust test cases (deterministic, distinct assets, case normalization, human readable formats, empty exchange/network, path sanitization)

**Step 2-5: Implement, run, commit**

---

### Task 9: Market data types and store interface

**Files:**
- Test: `ts/src/market-data/models.test.ts`
- Test: `ts/src/market-data/store.test.ts`
- Create: `ts/src/market-data/models.ts` (PriceKind, FxRateKind, PricePoint, FxRatePoint, AssetRegistryEntry)
- Create: `ts/src/market-data/store.ts` (MarketDataStore interface, NullMarketDataStore, MemoryMarketDataStore)

Port `src/market_data/models.rs` and `src/market_data/store.rs`.

**Step 1: Write tests** — MemoryMarketDataStore: put/get prices, put/get FX rates, case normalization for FX keys, get_all_prices, get_all_fx_rates, NullMarketDataStore returns nulls/empty

**Step 2-5: Implement, run, commit**

---

### Task 10: Storage interface and MemoryStorage

**Files:**
- Test: `ts/src/storage/memory-storage.test.ts`
- Create: `ts/src/storage/storage.ts` (Storage interface)
- Create: `ts/src/storage/memory-storage.ts`
- Create: `ts/src/storage/index.ts`

Port `src/storage/mod.rs` (trait) and `src/storage/memory.rs`. Storage interface has all CRUD methods for connections, accounts, balances, transactions. MemoryStorage implements it using Maps. Transaction deduplication (last-write-wins) in `getTransactions()` vs raw in `getTransactionsRaw()`.

**Step 1: Write tests** — CRUD for connections/accounts, balance snapshot append/latest, transaction dedup, latest balances for connection, error on missing connection

**Step 2-5: Implement, run, commit**

---

### Task 11: Storage lookup utilities

**Files:**
- Test: `ts/src/storage/lookup.test.ts`
- Create: `ts/src/storage/lookup.ts`

Port `src/storage/lookup.rs` — `findConnection()` and `findAccount()`. Look up by ID first, then case-insensitive name. Error on ambiguous name (multiple matches).

**Step 1: Write tests** — find by ID, find by name, error on duplicate names

**Step 2-5: Implement, run, commit**

---

### Task 12: JsonFileStorage

**Files:**
- Test: `ts/src/storage/json-file-storage.test.ts`
- Create: `ts/src/storage/json-file-storage.ts`

Port `src/storage/json_file.rs`. Directory layout:
```
data_dir/
  connections/{id}/
    connection.toml    (human-editable config)
    connection.json    (machine state)
    accounts/          (symlinks)
  accounts/{id}/
    account.json
    account_config.toml
    balances.jsonl
    transactions.jsonl
```

Connections: config in TOML, state in JSON. Accounts: JSON. Balances/transactions: JSONL (one JSON per line). Symlinks for connections by-name and accounts by connection.

**Step 1: Write tests** — save/load connection (TOML + JSON), save/load account, append/read balances JSONL, append/read transactions JSONL, deduplication, skip invalid entries, skip unsafe dirs, symlink creation

**Step 2-5: Implement, run, commit**

---

### Task 13: Credentials module

**Files:**
- Test: `ts/src/credentials/credentials.test.ts`
- Create: `ts/src/credentials/credential-store.ts` (CredentialStore interface)
- Create: `ts/src/credentials/config.ts` (CredentialConfig)
- Create: `ts/src/credentials/index.ts`

Port `src/credentials/mod.rs` — `CredentialStore` interface (get/set/supportsWrite). `CredentialConfig` for loading from TOML. Skip `PassCredentialStore` and `SessionCache` for now (they can be added later as they depend on external tools).

**Step 1: Write tests** — interface contract, config loading

**Step 2-5: Implement, run, commit**

---

### Task 14: Config module

**Files:**
- Test: `ts/src/config.test.ts`
- Create: `ts/src/config.ts`

Port `src/config.rs` — `Config`, `RefreshConfig`, `GitConfig`, `ResolvedConfig`. TOML parsing. Default values (reporting_currency: "USD", balance_staleness: "14d", price_staleness: "24h", auto_commit: false). Data dir resolution (relative to config file, absolute, or config dir default).

**Step 1: Write tests** — default values, load from TOML file, relative/absolute data dir resolution, load_or_default with missing file, refresh config parsing

**Step 2-5: Implement, run, commit**

---

### Task 15: Staleness module

**Files:**
- Test: `ts/src/staleness.test.ts`
- Create: `ts/src/staleness.ts`

Port `src/staleness.rs` — `StalenessCheck`, `resolveBalanceStaleness()`, `checkBalanceStalenessAt()`, `checkPriceStalenessAt()`. Resolution order: account config -> connection config -> global config. Age >= threshold is stale. Missing data is stale. Future timestamps are fresh.

**Step 1: Write tests** — stale when old, fresh when recent, stale when never synced, future timestamp is fresh, age equals threshold is stale, resolution order (account > connection > global), price staleness checks

**Step 2-5: Implement, run, commit**

---

### Task 16: Market Data Service

**Files:**
- Test: `ts/src/market-data/service.test.ts`
- Create: `ts/src/market-data/service.ts`
- Create: `ts/src/market-data/sources.ts` (source interfaces and routers)
- Create: `ts/src/market-data/provider.ts` (MarketDataSource interface)
- Create: `ts/src/market-data/index.ts`

Port `src/market_data/service.rs`, `src/market_data/sources.rs`, `src/market_data/provider.rs`.

`MarketDataService` — core service with store, provider, routers, lookback, quote staleness, clock injection. Methods: `priceFromStore()`, `priceClose()`, `priceCloseForce()`, `priceLatest()`, `fxClose()`, `fxCloseForce()`, `storePrice()`, `registerAsset()`.

Source interfaces: `EquityPriceSource`, `CryptoPriceSource`, `FxRateSource`. Routers: `EquityPriceRouter`, `CryptoPriceRouter`, `FxRateRouter` — try sources in order.

`MarketDataSource` legacy interface (`fetchPrice`, `fetchFxRate`).

**Step 1: Write tests** — lookback for missing prices, cached quote staleness, identity FX rate (same currency), store price idempotency, force refresh

**Step 2-5: Implement, run, commit**

---

### Task 17: Portfolio models and service

**Files:**
- Test: `ts/src/portfolio/portfolio.test.ts`
- Create: `ts/src/portfolio/models.ts`
- Create: `ts/src/portfolio/service.ts`
- Create: `ts/src/portfolio/index.ts`

Port `src/portfolio/models.rs` and `src/portfolio/service.rs`.

Types: `Grouping`, `PortfolioQuery`, `PortfolioSnapshot`, `AssetSummary`, `AccountHolding`, `AccountSummary`.

`PortfolioService` — takes storage + market data, `calculate(query)` returns snapshot. Aggregates balances by normalized asset, fetches valuations, handles FX conversion, builds asset and account summaries. Uses Decimal.js for arithmetic.

**Step 1: Write tests** — single currency holding (USD->USD = 1000), equity with FX conversion (AAPL * price * FX rate), detail holdings across accounts, case-insensitive asset merging, uses latest snapshot before date, zero backfill policy, carry-earliest backfill, historical uses close not quote

**Step 2-5: Implement, run, commit**

---

### Task 18: Change points module

**Files:**
- Test: `ts/src/portfolio/change-points.test.ts`
- Create: `ts/src/portfolio/change-points.ts`

Port `src/portfolio/change_points.rs` — `ChangePoint`, `ChangeTrigger`, `ChangePointCollector`, `Granularity`, `CoalesceStrategy`, `filterByGranularity()`, `filterByDateRange()`, `collectChangePoints()`.

**Step 1: Write tests** — collector tracks balance changes, merges same timestamp, sorts by timestamp, daily/weekly/monthly/yearly granularity filtering, date range filtering, zero-duration custom returns input

**Step 2-5: Implement, run, commit**

---

### Task 19: Sync types and interfaces

**Files:**
- Test: `ts/src/sync/sync.test.ts`
- Create: `ts/src/sync/synchronizer.ts` (Synchronizer, InteractiveAuth, AuthStatus interfaces)
- Create: `ts/src/sync/result.ts` (SyncResult, SyncedAssetBalance, SyncOutcome)
- Create: `ts/src/sync/context.ts` (SyncContext, AuthPrompter, AutoCommitter, FixedAuthPrompter, NoopAutoCommitter)
- Create: `ts/src/sync/orchestrator.ts` (SyncOrchestrator, PriceRefreshResult)
- Create: `ts/src/sync/index.ts`

Port sync module types and orchestration from `src/sync/`. This task covers the interfaces, result types, and orchestration logic. Actual synchronizer implementations (Chase, Schwab, Coinbase) are NOT included — they are platform-specific.

`SyncResult.save()` method persists accounts, balances, and transactions to storage. `SyncOrchestrator.ensurePrices()` refreshes prices for held assets.

**Step 1: Write tests** — SyncResult save persists data, orchestrator price refresh counting, auth prompter allow/deny

**Step 2-5: Implement, run, commit**

---

### Task 20: Git auto-commit module

**Files:**
- Test: `ts/src/git.test.ts`
- Create: `ts/src/git.ts`

Port `src/git.rs` — `tryAutoCommit()` function. Checks if data dir is a git repo, stages and commits if there are changes. Returns `AutoCommitOutcome` (committed, skippedNoChanges, skippedNotRepo).

**Step 1: Write tests** — commits when changes exist, skips when no changes, skips when not a repo

**Step 2-5: Implement, run, commit**

---

### Task 21: JSONL Market Data Store

**Files:**
- Test: `ts/src/market-data/jsonl-store.test.ts`
- Create: `ts/src/market-data/jsonl-store.ts`

Port `src/market_data/jsonl_store.rs` — file-based market data storage using JSONL files. Directory layout: `market_data/prices/{asset_id}/{date}.jsonl`, `market_data/fx/{base}-{quote}/{date}.jsonl`, `market_data/assets/{asset_id}.json`.

**Step 1: Write tests** — put/get prices, put/get FX rates, persistence across instances, idempotent writes

**Step 2-5: Implement, run, commit**

---

### Task 22: Library barrel export

**Files:**
- Create: `ts/src/index.ts`

Create the main barrel export that re-exports all public types, matching `src/lib.rs`.

**Step 1: Create index.ts with all public exports**

**Step 2: Verify typecheck passes**

Run: `cd ts && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add ts/src/index.ts
git commit -m "feat(ts): add barrel export for library"
```

---

## Execution Notes

**Module dependency order:**
1. Clock (no deps)
2. Duration (no deps)
3. Id (uuid)
4. IdGenerator (Id)
5. Asset (no deps)
6. Core models (Clock, Id, IdGenerator, Asset)
7. AssetId (Asset)
8. Market data types + store (AssetId)
9. Storage interface + MemoryStorage (models)
10. Storage lookup (Storage)
11. JsonFileStorage (Storage, models)
12. Credentials (interface only)
13. Config (Duration)
14. Staleness (Config, models, market data)
15. Market data service (store, sources, clock)
16. Portfolio (storage, market data, models)
17. Change points (storage, market data store)
18. Sync types (storage, models, market data)
19. Git (child_process)
20. JSONL market data store (market data types)
21. Barrel export

**Key design decisions:**
- Amounts always stored as strings, computed via `Decimal.js`
- Dates: `Date` for timestamps, `string` (YYYY-MM-DD) for NaiveDate equivalent
- JSON serialization uses snake_case field names to match Rust serde format (for data compatibility)
- Interfaces used for all trait equivalents (Storage, Clock, IdGenerator, CredentialStore, etc.)
- No external HTTP dependencies — provider implementations can be added separately
- `async`/`await` throughout for Storage and MarketDataStore operations
