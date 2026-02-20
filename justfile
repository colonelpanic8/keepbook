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

# Portfolio totals for each month over the last N months.
# Uses month-end snapshots (today for current month), then imputes missing per-account
# values by carrying the most recent prior non-zero value forward; if none exists,
# uses the next non-zero value backward.
# Example:
#   just history-monthly-totals
#   just history-monthly-totals 24
#   just history-monthly-totals 6
history-monthly-totals months='12':
    #!/usr/bin/env bash
    set -euo pipefail

    months="{{months}}"
    if ! [[ "$months" =~ ^[0-9]+$ ]] || [ "$months" -le 0 ]; then
      echo "months must be a positive integer (got: $months)" >&2
      exit 1
    fi

    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT

    for i in $(seq $((months - 1)) -1 0); do
      if [ "$i" -eq 0 ]; then
        date="$(date +%F)"
      else
        date="$(date -d "$(date +%Y-%m-01) -$i month +1 month -1 day" +%F)"
      fi

      snapshot="$({{keepbook_cmd}} portfolio snapshot --offline --date "$date" --group-by account)"
      printf '%s\n' "$snapshot" \
        | jq -c --arg date "$date" '{date: $date, by_account: (.by_account // [])}' \
        >> "$tmp"
    done

    jq -s '
      def decstr:
        tostring
        | if test("\\.") then sub("0+$"; "") | sub("\\.$"; "") else . end;

      def value_for($row; $id):
        ($row.by_account | map(select(.account_id == $id) | .value_in_base) | .[0] // null);

      def nonzero($v):
        (try ($v | tonumber) catch null) as $n | ($n != null and $n != 0);

      def fill_nonzero($vals):
        [range(0; ($vals | length)) as $i
          | ($vals[$i]) as $cur
          | if nonzero($cur) then $cur
            else
              (([range(0; $i) | $vals[.] | select(nonzero(.))] | last)
               // ([range($i + 1; ($vals | length)) | $vals[.] | select(nonzero(.))] | first)
               // $cur)
            end
        ];

      . as $rows
      | ($rows | map(.date)) as $dates
      | ($rows | map(.by_account[]?.account_id) | unique) as $ids
      | ($ids
         | map({
             key: .,
             value: fill_nonzero([$rows[] as $row | value_for($row; .)])
           })
         | from_entries) as $filled
      | [range(0; ($dates | length)) as $i
          | {
              date: $dates[$i],
              balance: ((([$ids[] as $id | (($filled[$id][$i] | tonumber?) // 0)] | add) * 100 | round / 100) | decstr),
              missing_account_count: ([$ids[] as $id
                | if value_for($rows[$i]; $id) == null then 1 else 0 end] | add)
            }
        ]
    ' "$tmp"

alias monthly-totals := history-monthly-totals

# Portfolio snapshot distilled to total and per-account base-currency totals.
# Extra snapshot args can be passed through, e.g.:
#   just snapshot-account-totals --date 2026-02-01
snapshot-account-totals *args:
    {{keepbook_cmd}} portfolio snapshot {{args}} --group-by account | jq '{currency, total_value_in_base_currency: .total_value, accounts_to_total_value_in_base_currency: ((.by_account // []) | map({key: "\(.connection_name)/\(.account_name)", value: (.value_in_base // null)}) | from_entries)}'
