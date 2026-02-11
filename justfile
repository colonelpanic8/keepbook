# keepbook development helpers

# Base command used to invoke keepbook. Override with:
#   KEEPBOOK_CMD='keepbook' just history-daily-balance
# Default uses cargo in this workspace.
keepbook_cmd := env_var_or_default("KEEPBOOK_CMD", "cargo run --")

# Portfolio history distilled to daily date/balance JSON objects.
# Extra CLI args can be passed through, e.g.:
#   just history-daily-balance -- --start 2026-01-01 --end 2026-02-01
history-daily-balance *args:
    {{keepbook_cmd}} portfolio history --granularity daily {{args}} | jq '.points | map({date, balance: .total_value})'

# Portfolio history distilled to tab-separated date/balance rows.
history-daily-balance-tsv *args:
    {{keepbook_cmd}} portfolio history --granularity daily {{args}} | jq -r '.points[] | "\(.date)\t\(.total_value)"'
