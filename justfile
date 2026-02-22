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
# Uses `portfolio history --granularity monthly` as the source of truth, then
# fills month slots from the nearest available historical point (prefer prior,
# otherwise next) so output always has exactly N rows.
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

    month_dates="$(mktemp)"
    trap 'rm -f "$month_dates"' EXIT

    today="$(date +%F)"
    start="$(date -d "$(date +%Y-%m-01) -$((months - 1)) month" +%F)"
    history_points="$(
      {{keepbook_cmd}} portfolio history \
        --granularity monthly \
        --start "$start" \
        --end "$today" \
      | jq -c '.points'
    )"

    for i in $(seq $((months - 1)) -1 0); do
      if [ "$i" -eq 0 ]; then
        date="$(date +%F)"
      else
        date="$(date -d "$(date +%Y-%m-01) -$i month +1 month -1 day" +%F)"
      fi
      printf '%s\n' "$date" >> "$month_dates"
    done

    jq -n \
      --argjson history_points "$history_points" \
      --rawfile month_dates "$month_dates" '
      def decstr:
        tostring
        | if test("\\.") then sub("0+$"; "") | sub("\\.$"; "") else . end;

      def fmt_balance:
        tostring as $s
        | (if ($s | startswith("-")) then "-" else "" end) as $sign
        | ($s | ltrimstr("-")) as $abs
        | ($abs | split(".")) as $parts
        | ($parts[0] | gsub("(?<=[0-9])(?=(?:[0-9]{3})+$)"; ",")) as $int
        | ($parts[1] // "") as $frac
        | if $frac == "" then "\($sign)\($int)" else "\($sign)\($int).\($frac)" end;

      def fmt2:
        tostring as $s
        | if $s == "N/A" then $s
          elif ($s | contains(".")) then
            ($s | split(".")) as $parts
            | ($parts[0]) as $int
            | ($parts[1] // "") as $frac
            | if ($frac | length) == 0 then "\($int).00"
              elif ($frac | length) == 1 then "\($int).\($frac)0"
              else "\($int).\($frac[0:2])"
              end
          else
            "\($s).00"
          end;

      def row_for_date($rows; $d):
        (([$rows[] | select(.date <= $d)] | last)
          // ([$rows[] | select(.date >= $d)] | first));

      ($month_dates | split("\n") | map(select(length > 0))) as $dates
      | ($history_points // []) as $rows
      | [ $dates[] as $d
          | (row_for_date($rows; $d)) as $row
          | {
              date: $d,
              balance: (($row.total_value // "0") | tostring)
            }
        ] as $filled
      | [range(0; ($filled | length)) as $i
          | ($filled[$i]) as $row
          | $row.balance as $cur
          | (if $i > 0 then $filled[$i - 1].balance else null end) as $prev
          | ((($cur | tonumber?) // 0) | decstr) as $raw_balance
          | {
              date: $row.date,
              balance: ($raw_balance | fmt_balance),
              formatted_balance: ($raw_balance | fmt_balance),
              raw_balance: $raw_balance,
              percentage_change_from_previous:
                (if $prev == null then
                   null
                 elif ((($prev | tonumber?) // 0) == 0) then
                   "N/A"
                 else
                   ((((($cur | tonumber?) // 0) - (($prev | tonumber?) // 0))
                     / (($prev | tonumber?) // 0)
                     * 100
                     * 100
                    | round) / 100 | fmt2)
                 end)
            }
        ]
    '

alias monthly-totals := history-monthly-totals

# Portfolio totals for each quarter over the last N quarters.
# Uses monthly portfolio history and resolves each quarter date to the nearest
# available point (prefer prior, otherwise next).
# Example:
#   just history-quarterly-totals
#   just history-quarterly-totals 12
history-quarterly-totals quarters='8':
    #!/usr/bin/env bash
    set -euo pipefail

    quarters="{{quarters}}"
    if ! [[ "$quarters" =~ ^[0-9]+$ ]] || [ "$quarters" -le 0 ]; then
      echo "quarters must be a positive integer (got: $quarters)" >&2
      exit 1
    fi

    quarter_dates="$(mktemp)"
    trap 'rm -f "$quarter_dates"' EXIT

    today="$(date +%F)"
    current_month="$(date +%m)"
    current_quarter_start_month=$(( ((10#$current_month - 1) / 3) * 3 + 1 ))
    current_quarter_start="$(date +%Y)-$(printf '%02d' "$current_quarter_start_month")-01"
    start="$(date -d "$current_quarter_start -$(((quarters - 1) * 3)) month" +%F)"
    history_points="$(
      {{keepbook_cmd}} portfolio history \
        --granularity monthly \
        --start "$start" \
        --end "$today" \
      | jq -c '.points'
    )"

    for i in $(seq $((quarters - 1)) -1 0); do
      if [ "$i" -eq 0 ]; then
        date="$today"
      else
        date="$(date -d "$current_quarter_start -$((i * 3)) month +3 month -1 day" +%F)"
      fi
      printf '%s\n' "$date" >> "$quarter_dates"
    done

    jq -n \
      --argjson history_points "$history_points" \
      --rawfile quarter_dates "$quarter_dates" '
      def decstr:
        tostring
        | if test("\\.") then sub("0+$"; "") | sub("\\.$"; "") else . end;

      def fmt_balance:
        tostring as $s
        | (if ($s | startswith("-")) then "-" else "" end) as $sign
        | ($s | ltrimstr("-")) as $abs
        | ($abs | split(".")) as $parts
        | ($parts[0] | gsub("(?<=[0-9])(?=(?:[0-9]{3})+$)"; ",")) as $int
        | ($parts[1] // "") as $frac
        | if $frac == "" then "\($sign)\($int)" else "\($sign)\($int).\($frac)" end;

      def fmt2:
        tostring as $s
        | if $s == "N/A" then $s
          elif ($s | contains(".")) then
            ($s | split(".")) as $parts
            | ($parts[0]) as $int
            | ($parts[1] // "") as $frac
            | if ($frac | length) == 0 then "\($int).00"
              elif ($frac | length) == 1 then "\($int).\($frac)0"
              else "\($int).\($frac[0:2])"
              end
          else
            "\($s).00"
          end;

      def row_for_date($rows; $d):
        (([$rows[] | select(.date <= $d)] | last)
          // ([$rows[] | select(.date >= $d)] | first));

      ($quarter_dates | split("\n") | map(select(length > 0))) as $dates
      | ($history_points // []) as $rows
      | [ $dates[] as $d
          | (row_for_date($rows; $d)) as $row
          | {
              date: $d,
              balance: (($row.total_value // "0") | tostring)
            }
        ] as $filled
      | [range(0; ($filled | length)) as $i
          | ($filled[$i]) as $row
          | $row.balance as $cur
          | (if $i > 0 then $filled[$i - 1].balance else null end) as $prev
          | ((($cur | tonumber?) // 0) | decstr) as $raw_balance
          | {
              date: $row.date,
              balance: ($raw_balance | fmt_balance),
              formatted_balance: ($raw_balance | fmt_balance),
              raw_balance: $raw_balance,
              percentage_change_from_previous:
                (if $prev == null then
                   null
                 elif ((($prev | tonumber?) // 0) == 0) then
                   "N/A"
                 else
                   ((((($cur | tonumber?) // 0) - (($prev | tonumber?) // 0))
                     / (($prev | tonumber?) // 0)
                     * 100
                     * 100
                    | round) / 100 | fmt2)
                 end)
            }
        ]
    '

alias quarterly-totals := history-quarterly-totals

# Portfolio snapshot distilled to total and per-account base-currency totals.
# Extra snapshot args can be passed through, e.g.:
#   just snapshot-account-totals --date 2026-02-01
snapshot-account-totals *args:
    {{keepbook_cmd}} portfolio snapshot {{args}} --group-by account | jq '{currency, total_value_in_base_currency: .total_value, accounts_to_total_value_in_base_currency: ((.by_account // []) | map({key: "\(.connection_name)/\(.account_name)", value: (.value_in_base // null)}) | from_entries)}'
