# keepbook

## Overview

Keepbook is a local-first personal finance toolkit. Data is stored in plain JSON/TOML/JSONL files. The in-repo implementation is Rust, with Dioxus for app UI surfaces.

## Compatibility Rules

- **CLI command structure**: command names, flags, arguments, help text
- **JSON output format**: field names (snake_case), field types, null vs omitted semantics
- **Timestamp formatting**: `+00:00` suffix for app output types (`formatRfc3339`), `Z` suffix for library types (`formatChronoSerde`)
- **Decimal formatting**: trailing zeros stripped (Rust `Decimal::normalize().to_string()`, TS `decStr()`)
- **Asset serialization**: tagged union with `type` field, snake_case fields, optional fields omitted when absent
- **Storage format**: JSON/JSONL file structure, field names, serialization rules
- **Business logic**: portfolio valuation, change point collection, balance aggregation, account counting

### When making changes

1. Update the Rust app/library behavior.
2. Write/update tests that verify exact JSON output when output formats change.
3. Run `cargo test` and any narrower validation relevant to the touched crates.

## UI Value Sourcing

UI code must not reimplement keepbook business calculations from raw storage data.
When a UI needs derived financial values such as net worth, portfolio totals,
account counts, balance aggregation, spending totals, history points, or change
summaries, source those values from the primary headless keepbook APIs/services
that already encode the canonical business logic.

Examples:
- Use portfolio snapshot/history outputs for net worth and chart values instead
  of summing raw balances in a UI component.
- Use list/spending/portfolio app-layer outputs when ignore rules,
  `exclude_from_portfolio`, price lookup, formatting, or account validity rules
  can affect the answer.
- If the current API does not expose the needed derived value, extend the
  headless Rust app-layer output first, then consume that output in the UI.

This applies to all UI surfaces, including Dioxus, Expo/React Native, web
components, tray/menu views, and any future frontend.

## Development Environment

- Run repository commands in the direnv-provided environment.
- If the current shell has not already loaded direnv, use `direnv exec . <command>` from the repository root instead of `nix develop --command <command>`.
- Prefer direct commands such as `cargo test` only when the active shell is already inside the direnv context.

## Project Structure

```
src/                    # Rust implementation
  app.rs                # CLI command handlers (THE reference for output formats)
  storage/              # Storage trait + JSON file implementation
  sync/                 # Synchronizer traits, orchestration, auth flows
  market_data/          # Store, source adapters, routers, service builder
  portfolio/            # Valuation, history, change-point logic
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
auto_push = false
```

## Testing

- Rust: `cargo test`

Tests should pass before any push.

## Key Reference Files

When modifying output formats, consult:
- **Rust**: `src/app.rs` (output structs and command handlers)
- **Contract tests**: `tests/contracts.rs` and `contracts/*.json`
