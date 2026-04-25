# Keepbook CLI Reference For Data Operations

All commands emit JSON unless they launch an interactive UI/browser flow. Use `--config <path>` to target a specific keepbook config.

## Global Options

```bash
keepbook --config ./keepbook.toml <command>
keepbook --git-merge-master <command>
keepbook --skip-git-merge-master <command>
keepbook --schwab-username "$USER" --schwab-password "$PASS" auth schwab login
```

## Discovery

```bash
keepbook config
keepbook list connections
keepbook list accounts
keepbook list balances
keepbook list transactions --start 2026-01-01 --end 2026-04-25
keepbook list price-sources
keepbook list all
```

Transaction listing defaults to a recent range. Add `--include-ignored` when auditing ignore rules.

## Manual Data Changes

```bash
keepbook add connection "Manual" --synchronizer manual
keepbook add account --connection <connection-id> "Checking" --tag cash
keepbook remove connection <connection-id>
```

Balances are complete account snapshots at the current timestamp:

```bash
keepbook set balance --account <account-id-or-name> --asset USD --amount 1234.56
keepbook set balance --account <account-id-or-name> --asset equity:AAPL --amount 10 --cost-basis 1200
```

Account config currently supports balance backfill policy:

```bash
keepbook set account-config --account <account-id-or-name> --balance-backfill zero
keepbook set account-config --account <account-id-or-name> --clear-balance-backfill
```

Transaction annotations are append-only patches:

```bash
keepbook set transaction --account <account-id> --transaction <tx-id> --category Dining --note "Team lunch"
keepbook set transaction --account <account-id> --transaction <tx-id> --tag reimbursable --tag work
keepbook set transaction --account <account-id> --transaction <tx-id> --tags-empty
keepbook set transaction --account <account-id> --transaction <tx-id> --clear-category --clear-note
keepbook set transaction --account <account-id> --transaction <tx-id> --effective-date 2026-02-01
```

## Sync, Auth, And Import

```bash
keepbook auth schwab login [connection-id-or-name]
keepbook auth chase login [connection-id-or-name]
keepbook sync connection <connection-id-or-name> --if-stale --transactions auto
keepbook sync connection <connection-id-or-name> --transactions full
keepbook sync all --if-stale
keepbook sync prices all --force
keepbook sync prices connection <connection-id-or-name> --quote-staleness 6h
keepbook sync prices account <account-id-or-name>
keepbook sync symlinks
keepbook sync recompact
keepbook sync backfill-metadata
keepbook import schwab transactions --account <account-id-or-name> /path/to/export.json
```

Use `--transactions auto` for ordinary syncs and `--transactions full` for deliberate backfills.

Market data backfill:

```bash
keepbook market-data fetch --account <account-id-or-name> --start 2026-01-01 --end 2026-04-25
keepbook market-data fetch --connection <connection-id-or-name> --interval monthly --lookback-days 7
keepbook market-data fetch --currency USD --request-delay-ms 250
keepbook market-data fetch --no-fx
```

## Portfolio And Spending Reports

```bash
keepbook portfolio snapshot --offline
keepbook portfolio snapshot --currency USD --date 2026-04-25 --group-by both
keepbook portfolio snapshot --dry-run
keepbook portfolio snapshot --force-refresh
keepbook portfolio history --start 2026-01-01 --end 2026-04-25 --granularity monthly
keepbook portfolio change-points --start 2026-01-01 --granularity daily --no-include-prices
```

Spending:

```bash
keepbook spending --period monthly --group-by category --start 2026-01-01 --end 2026-04-25
keepbook spending --period weekly --week-start monday --status posted+pending
keepbook spending --period custom --bucket 14d --period-alignment end-bound
keepbook spending --connection <connection-id-or-name> --direction net
keepbook spending-categories --top 20 --include-empty
```

Supported spending options include `--currency`, `--tz`, `--account`, `--connection`, `--status posted|posted+pending|all`, `--direction outflow|inflow|net`, `--group-by none|category|merchant|account|tag`, `--lookback-days`, and `--include-noncurrency`.

## TUI

```bash
keepbook tui --view transactions
keepbook tui --view net-worth --net-worth-interval monthly
```
