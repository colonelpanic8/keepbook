# Keepbook Data Directory

This directory contains keepbook personal finance data. The goal in this tree is to operate and repair data, not modify keepbook source code.

## What Keepbook Is

Keepbook is a local-first personal finance toolkit. Data is stored as readable TOML, JSON, and JSONL files. The CLI emits JSON and should be preferred for mutations.

## First Commands

```bash
keepbook --config ./keepbook.toml config
keepbook --config ./keepbook.toml list accounts
keepbook --config ./keepbook.toml list balances
keepbook --config ./keepbook.toml list transactions
```

If `keepbook.toml` is not in this directory, locate the intended config first and use `--config <path>`.

## Common Operations

```bash
keepbook --config ./keepbook.toml set balance --account <account-id-or-name> --asset USD --amount 1234.56
keepbook --config ./keepbook.toml set transaction --account <account-id> --transaction <tx-id> --category Dining
keepbook --config ./keepbook.toml portfolio snapshot --offline
keepbook --config ./keepbook.toml spending --period monthly --group-by category
keepbook --config ./keepbook.toml sync connection <connection-id-or-name> --if-stale
keepbook --config ./keepbook.toml sync symlinks
```

## Safety

- Check `git status` before changes and preserve unrelated edits.
- Use CLI commands for balances, transaction annotations, syncs, reports, and symlink rebuilds.
- Treat `transactions.jsonl`, `balances.jsonl`, and `transaction_annotations.jsonl` as append-oriented logs.
- Do not commit credentials or hardcode secrets. Search `pass` for credentials and provide them at runtime.
- Run `jq` validation after manual JSON/JSONL edits.

## More Context

If available, use the `keepbook-data` skill. In this repo, its source lives at:

`agents/keepbook-data/skills/keepbook-data/SKILL.md`
