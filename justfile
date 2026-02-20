# keepbook development helpers

# Base command used to invoke keepbook. Override with:
#   KEEPBOOK_CMD='keepbook' just history-daily-balance
# Default uses cargo in this workspace and targets the keepbook binary.
keepbook_cmd := env_var_or_default("KEEPBOOK_CMD", "cargo run --bin keepbook --")
keepbook_tray_cmd := env_var_or_default(
  "KEEPBOOK_TRAY_CMD",
  "cargo run --features tray --bin keepbook-sync-daemon --",
)

# Run the development keepbook binary with arbitrary args.
# Example:
#   just kb portfolio snapshot
#   just -- kb --help
kb *args:
    {{keepbook_cmd}} {{args}}

# Run the sync daemon with tray UI.
# Example:
#   just run-tray
#   just run-tray -- --help
run-tray *args:
    {{keepbook_tray_cmd}} {{args}}

# Portfolio history distilled to daily date/balance JSON objects.
# Extra CLI args can be passed through, e.g.:
#   just history-daily-balance -- --start 2026-01-01 --end 2026-02-01
history-daily-balance *args:
    {{keepbook_cmd}} portfolio history --granularity daily {{args}} | jq '.points | map({date, balance: .total_value})'

# Portfolio history distilled to tab-separated date/balance rows.
history-daily-balance-tsv *args:
    {{keepbook_cmd}} portfolio history --granularity daily {{args}} | jq -r '.points[] | "\(.date)\t\(.total_value)"'

# Portfolio snapshot distilled to total and per-account base-currency totals.
# Extra snapshot args can be passed through, e.g.:
#   just snapshot-account-totals --date 2026-02-01
snapshot-account-totals *args:
    {{keepbook_cmd}} portfolio snapshot {{args}} --group-by account | jq '{currency, total_value_in_base_currency: .total_value, accounts_to_total_value_in_base_currency: ((.by_account // []) | map({key: "\(.connection_name)/\(.account_name)", value: (.value_in_base // null)}) | from_entries)}'
