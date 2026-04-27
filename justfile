# keepbook development helpers

set positional-arguments := true

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
    {{keepbook_cmd}} "$@"

# Run the sync daemon with tray UI.
# Example:
#   just run-tray
#   just run-tray -- --help
run-tray *args:
    {{keepbook_tray_cmd}} "$@"

# Build the Dioxus client as an iOS simulator app bundle.
dioxus-ios-build *args:
    #!/usr/bin/env bash
    set -euo pipefail

    unset SDKROOT DEVELOPER_DIR
    ios_sdk="$(/usr/bin/xcrun --sdk iphonesimulator --show-sdk-path)"
    ios_clang="$(/usr/bin/xcrun --sdk iphonesimulator --find clang)"

    export PATH="/usr/bin:$PATH"
    export CC_aarch64_apple_ios_sim="$ios_clang"
    export CFLAGS_aarch64_apple_ios_sim="-isysroot $ios_sdk -mios-simulator-version-min=13.0"
    export CXXFLAGS_aarch64_apple_ios_sim="-isysroot $ios_sdk -mios-simulator-version-min=13.0"
    export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER="$ios_clang"
    export IPHONEOS_DEPLOYMENT_TARGET=13.0

    dx build --ios --package keepbook-dioxus --no-default-features --features mobile "$@"

# Build the Dioxus client as an Android debug APK.
dioxus-android-build *args:
    nix develop .#android --command dx build --android --package keepbook-dioxus --no-default-features --features mobile "$@"
    find target/dx/keepbook-dioxus/debug/android -path '*/build/outputs/apk/*.apk' -print

# Build the Dioxus client as an Android release APK.
dioxus-android-release *args:
    nix develop .#android --command dx bundle --android --release --package keepbook-dioxus --no-default-features --features mobile "$@"
    nix develop .#android --command bash -lc 'cd target/dx/keepbook-dioxus/release/android/app && ./gradlew :app:assembleRelease --no-daemon --console plain'
    find target/dx/keepbook-dioxus/release/android \( -path '*/build/outputs/apk/release/*.apk' -o -path '*/build/outputs/bundle/release/*.aab' \) -print

# Portfolio history distilled to daily date/balance JSON objects.
# Extra CLI args can be passed through, e.g.:
#   just history-daily-balance -- --start 2026-01-01 --end 2026-02-01
history-daily-balance *args:
    {{keepbook_cmd}} portfolio history --granularity daily "$@" | jq '.points | map({date, balance: .total_value})'

# Portfolio history distilled to tab-separated date/balance rows.
history-daily-balance-tsv *args:
    {{keepbook_cmd}} portfolio history --granularity daily "$@" | jq -r '.points[] | "\(.date)\t\(.total_value)"'

# Historical latent capital gains tax burden as tab-separated date/value rows.
# Extra CLI args can override defaults, e.g.:
#   just history-tax-burden -- --granularity weekly --start 2025-01-01
history-tax-burden *args:
    {{keepbook_cmd}} portfolio history --account virtual:latent_capital_gains_tax --start -1y --granularity monthly "$@" | jq -r '.points[] | "\(.date)\t\(.total_value)"'

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

    date_adjust() {
      local base="$1"
      shift

      if date -d "$base $*" +%F >/dev/null 2>&1; then
        date -d "$base $*" +%F
        return
      fi

      local args=()
      while [ "$#" -gt 0 ]; do
        local amount="$1"
        local unit="$2"
        shift 2

        case "$unit" in
          day|days)
            args+=("-v${amount}d")
            ;;
          month|months)
            args+=("-v${amount}m")
            ;;
          *)
            echo "unsupported date unit: $unit" >&2
            return 1
            ;;
        esac
      done

      date -j -f %F "$base" "${args[@]}" +%F
    }

    months="{{months}}"
    if ! [[ "$months" =~ ^[0-9]+$ ]] || [ "$months" -le 0 ]; then
      echo "months must be a positive integer (got: $months)" >&2
      exit 1
    fi

    month_dates="$(mktemp)"
    trap 'rm -f "$month_dates"' EXIT

    today="$(date +%F)"
    current_month_start="$(date +%Y-%m-01)"
    start="$(date_adjust "$current_month_start" "-$((months - 1))" month)"
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
        date="$(date_adjust "$current_month_start" "-$i" month +1 month -1 day)"
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

    date_adjust() {
      local base="$1"
      shift

      if date -d "$base $*" +%F >/dev/null 2>&1; then
        date -d "$base $*" +%F
        return
      fi

      local args=()
      while [ "$#" -gt 0 ]; do
        local amount="$1"
        local unit="$2"
        shift 2

        case "$unit" in
          day|days)
            args+=("-v${amount}d")
            ;;
          month|months)
            args+=("-v${amount}m")
            ;;
          *)
            echo "unsupported date unit: $unit" >&2
            return 1
            ;;
        esac
      done

      date -j -f %F "$base" "${args[@]}" +%F
    }

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
    start="$(date_adjust "$current_quarter_start" "-$(((quarters - 1) * 3))" month)"
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
        date="$(date_adjust "$current_quarter_start" "-$((i * 3))" month +3 month -1 day)"
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

# Spending totals by category over a time range.
# Extra spending args can be passed through, e.g.:
#   just spending-by-category -- --start 2026-01-01 --end 2026-02-01
#   just spending-by-category -- --connection Chase --status posted+pending
spending-by-category *args:
    #!/usr/bin/env bash
    set -euo pipefail

    date_adjust() {
      local base="$1"
      shift

      if date -d "$base $*" +%F >/dev/null 2>&1; then
        date -d "$base $*" +%F
        return
      fi

      local args=()
      while [ "$#" -gt 0 ]; do
        local amount="$1"
        local unit="$2"
        shift 2

        case "$unit" in
          day|days)
            args+=("-v${amount}d")
            ;;
          month|months)
            args+=("-v${amount}m")
            ;;
          *)
            echo "unsupported date unit: $unit" >&2
            return 1
            ;;
        esac
      done

      date -j -f %F "$base" "${args[@]}" +%F
    }

    start=""
    end=""
    expect_value=""

    for arg in "$@"; do
      if [ -n "$expect_value" ]; then
        if [ "$expect_value" = "start" ]; then
          start="$arg"
        else
          end="$arg"
        fi
        expect_value=""
        continue
      fi

      case "$arg" in
        --start)
          expect_value="start"
          ;;
        --start=*)
          start="${arg#--start=}"
          ;;
        --end)
          expect_value="end"
          ;;
        --end=*)
          end="${arg#--end=}"
          ;;
      esac
    done

    if [ -z "$end" ]; then
      end="$(date +%F)"
    fi

    if [ -z "$start" ]; then
      start="$(date_adjust "$end" -30 days)"
    fi

    {{keepbook_cmd}} spending-categories "$@" --period range --start "$start" --end "$end" \
      | jq '{meta: {start_date, end_date}, total, categories: ((.periods[0]?.breakdown // []) | map({key, value: .total}) | from_entries)}'

# Portfolio snapshot distilled to total and per-account base-currency totals.
# Extra snapshot args can be passed through, e.g.:
#   just snapshot-account-totals --date 2026-02-01
snapshot-account-totals *args:
    {{keepbook_cmd}} portfolio snapshot "$@" --group-by account | jq '{currency, total_value_in_base_currency: .total_value, accounts_to_total_value_in_base_currency: ((.by_account // []) | map({key: "\(.connection_name)/\(.account_name)", value: (.value_in_base // null)}) | from_entries)}'
