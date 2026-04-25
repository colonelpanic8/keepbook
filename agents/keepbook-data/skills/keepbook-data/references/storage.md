# Keepbook Storage Reference

The resolved `data_dir` stores plain JSON, TOML, and JSONL files.

```text
data/
  keepbook.toml                 # common when config and data live together
  connections/
    by-name/                    # symlinks to connection dirs
    <connection-id>/
      connection.toml           # human-declared config
      connection.json           # machine-managed state
      accounts/                 # symlinks to account dirs
  accounts/
    <account-id>/
      account.json
      account_config.toml       # optional
      balances.jsonl
      transactions.jsonl
      transaction_annotations.jsonl
  assets/
    index.jsonl
  prices/
    <asset-id>/
      <year>.jsonl
  fx/
    <BASE>-<QUOTE>/
      <year>.jsonl
  price_sources/
    <source-name>/
      source.toml
```

## File Semantics

- `connection.toml`: human-editable connection name, synchronizer, optional credentials, staleness overrides.
- `connection.json`: machine state: id, status, last sync, account ids, synchronizer data.
- `account.json`: machine-readable account identity, connection id, tags, active flag, synchronizer data.
- `account_config.toml`: human-editable per-account overrides.
- `balances.jsonl`: append-only balance snapshots. Each line is a complete account state at one timestamp.
- `transactions.jsonl`: append-only raw/source transactions. Read path dedupes transaction ids with last-write-wins.
- `transaction_annotations.jsonl`: append-only annotation patches. Use this instead of editing raw transactions.
- `assets/index.jsonl`, `prices/**`, `fx/**`: market data registry and cached historical quotes/rates.

## Asset Syntax

CLI asset arguments:

```text
USD
equity:AAPL
crypto:BTC
```

Serialized assets are tagged JSON:

```json
{"type":"currency","iso_code":"USD"}
{"type":"equity","ticker":"AAPL"}
{"type":"crypto","symbol":"BTC"}
```

Optional `exchange` on equities and `network` on crypto are omitted when absent.

## Safe Editing Checklist

1. Prefer CLI writes.
2. If editing TOML, run `keepbook config` or the relevant list/report command afterward.
3. If editing JSON/JSONL manually, validate syntax:
   ```bash
   jq empty accounts/<account-id>/account.json
   jq -c . accounts/<account-id>/transactions.jsonl >/dev/null
   ```
4. Rebuild links after identity/name changes:
   ```bash
   keepbook sync symlinks
   ```
5. Compact only when intended:
   ```bash
   keepbook sync recompact
   ```

## Transaction Annotation Patch Semantics

In `transaction_annotations.jsonl`, absent means "no change", JSON `null` means "clear this field", and a value means "set this field". Supported fields are `description`, `note`, `category`, `tags`, and `effective_date`.

Prefer:

```bash
keepbook set transaction --account <account-id> --transaction <tx-id> --clear-category
```

over writing patches by hand.
