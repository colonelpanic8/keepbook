# keepbook

## Overview

Keepbook is a local-first personal finance toolkit. Data is stored in plain JSON/TOML/JSONL files. The core has **two implementations that must stay in sync**:

1. **Rust** (`src/`, `Cargo.toml`) - the original CLI and library
2. **TypeScript** (`ts/`, `ts/package.json`) - a port of the library and CLI

## Dual-Implementation Rules

**Any change to business logic, output format, or data structures must be applied to both implementations.**

### What must stay in sync

- **CLI command structure**: command names, flags, arguments, help text
- **JSON output format**: field names (snake_case), field types, null vs omitted semantics
- **Timestamp formatting**: `+00:00` suffix for app output types (`formatRfc3339`), `Z` suffix for library types (`formatChronoSerde`)
- **Decimal formatting**: trailing zeros stripped (Rust `Decimal::normalize().to_string()`, TS `decStr()`)
- **Asset serialization**: tagged union with `type` field, snake_case fields, optional fields omitted when absent
- **Storage format**: JSON/JSONL file structure, field names, serialization rules
- **Business logic**: portfolio valuation, change point collection, balance aggregation, account counting

### null vs undefined (critical)

Rust uses `#[serde(skip_serializing_if)]` to omit fields. In TypeScript:
- Fields **without** `skip_serializing_if` in Rust -> use `| null` in TS (serialized as `null`)
- Fields **with** `skip_serializing_if` in Rust -> use `?:` in TS (omitted from JSON)

See `ts/src/app/types.ts` for the complete mapping.

### When making changes

1. Implement in one language first
2. Write/update tests that verify exact JSON output
3. Port to the other language
4. Run both test suites: `cargo test` (Rust) and `cd ts && yarn test` (TS)
5. Diff JSON output of both CLIs against the same data directory to verify compatibility

## TypeScript Package Manager

- Prefer `yarn` for TypeScript workflows in `ts/` (install, build, test, and CLI execution).
- Use `npm`/`npx` only when explicitly required.

## Project Structure

```
src/                    # Rust implementation
  app.rs                # CLI command handlers (THE reference for output formats)
  storage/              # Storage trait + JSON file implementation
  sync/                 # Synchronizer traits, orchestration, auth flows
  market_data/          # Store, source adapters, routers, service builder
  portfolio/            # Valuation, history, change-point logic

ts/                     # TypeScript implementation
  src/
    app/                # CLI command handlers (mirrors Rust src/app.rs)
      format.ts         # Timestamp/decimal formatting utilities
      types.ts          # Output type interfaces (mirrors Rust app structs)
      config.ts         # Config loading
      list.ts           # List commands
      mutations.ts      # Add/remove/set commands
      portfolio.ts      # Portfolio snapshot/history/change-points
      sync.ts           # Sync stubs
    cli/
      main.ts           # Commander.js CLI entry point
    models/             # Core domain types (mirrors Rust models)
    storage/            # Storage trait + implementations
    market-data/        # Market data store + service
    portfolio/          # Portfolio service + change points
```

## CLI Commands

Both CLIs emit JSON. Command structure:

- `config`
- `add connection|account`
- `remove connection`
- `set balance`
- `list connections|accounts|balances|transactions|price-sources|all`
- `sync connection|all|prices|symlinks`
- `auth schwab|chase login`
- `market-data fetch`
- `portfolio snapshot|history|change-points`

## Configuration

Default config path: `./keepbook.toml`, then XDG data dir fallback.

```toml
reporting_currency = "USD"

[refresh]
balance_staleness = "14d"
price_staleness = "24h"

[git]
auto_commit = false
```

## Testing

- Rust: `cargo test`
- TypeScript: `cd ts && yarn test`

Both must pass before any push.

## Key Reference Files

When modifying output formats, consult:
- **Rust**: `src/app.rs` (output structs and command handlers)
- **TypeScript**: `ts/src/app/types.ts` (output interfaces) and `ts/src/app/format.ts` (formatting)
- **Integration tests**: `ts/src/app/integration.test.ts` (JSON compatibility verification)
