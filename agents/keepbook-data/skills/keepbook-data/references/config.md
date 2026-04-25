# Keepbook Config Reference

Keepbook config is TOML. Default lookup:

1. `./keepbook.toml` if present in the current directory.
2. `~/.local/share/keepbook/keepbook.toml`.

If `data_dir` is relative, it is resolved relative to the config file directory. If omitted, the config directory is the data directory.

## Example

```toml
# Optional; relative paths are resolved from this file's directory.
data_dir = "./data"

reporting_currency = "USD"

[display]
currency_decimals = 2
currency_grouping = true
currency_symbol = "$"
currency_fixed_decimals = true

[refresh]
balance_staleness = "14d"
price_staleness = "24h"

[history]
allow_future_projection = false
# lookback_days = 7

[tray]
history_points = 17
history_spec = ["last 4 days", "1 week ago", "2 weeks ago", "last 12 months"]
spending_windows_days = [7, 30, 90, 365]
transaction_count = 30

[spending]
ignore_accounts = []
ignore_connections = []
ignore_tags = []

[portfolio.latent_capital_gains_tax]
enabled = false
# rate = 0.23
account_name = "Latent Capital Gains Tax"

[ignore]
transaction_rules = []

[git]
auto_commit = false
# When omitted, auto_push defaults to auto_commit.
auto_push = false
merge_master_before_command = false
```

## Ignore Rules

`[spending]` ignore lists are simple account/connection/tag scopes used by default portfolio spending and transaction views.

`[[ignore.transaction_rules]]` entries use regex patterns. All configured fields in a rule must match for the transaction to be ignored.

```toml
[[ignore.transaction_rules]]
account_name = "Checking"
description = "PAYROLL|TRANSFER"
status = "posted"
```

Available fields: `account_id`, `account_name`, `connection_id`, `connection_name`, `synchronizer`, `description`, `status`, `amount`.

## Connection Config

Connections live under `connections/<connection-id>/connection.toml`.

```toml
name = "Schwab"
synchronizer = "schwab"
balance_staleness = "12h"

# Prefer pass/env indirection over inline secrets.
# [credentials]
# ...
```

Connection machine state is in `connection.json`; do not edit it unless repairing data deliberately.

## Account Config

Optional account config lives at `accounts/<account-id>/account_config.toml`.

```toml
balance_staleness = "7d"
balance_backfill = "zero" # none | zero | carry_earliest
exclude_from_portfolio = true
```

Use `keepbook set account-config` for supported fields when possible.

## Price Sources

Price sources live under `price_sources/<source-name>/source.toml`.

Implemented source types include equities (`eodhd`, `twelve_data`, `alpha_vantage`, `marketstack`), crypto (`coingecko`, `cryptocompare`, `coincap`), and FX (`frankfurter`).

Keep API keys in `pass`, environment variables, or supported credential references. Do not commit raw secrets.
