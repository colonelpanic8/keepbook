---
name: keepbook-data
description: Use when manipulating an existing keepbook personal finance data directory with the keepbook CLI, jq, TOML/JSON/JSONL inspection, config edits, sync/auth commands, transaction annotations, portfolio/spending reports, or data hygiene tasks. This skill is for operating data, not changing keepbook source code.
---

# Keepbook Data Operations

Use this skill for working in or against a keepbook data directory. Keepbook is a local-first personal finance toolkit: configuration is TOML, durable records are JSON/JSONL, and the CLI emits JSON.

## First Steps

1. Identify the config file and resolved data directory:
   ```bash
   keepbook --config /path/to/keepbook.toml config
   ```
   If no config is given, keepbook checks `./keepbook.toml`, then `~/.local/share/keepbook/keepbook.toml`.
2. Prefer keepbook CLI mutations over manual file edits. Manual edits are acceptable for config files and careful append-only JSONL repair, but validate afterward.
3. Before changing data, inspect `git status` in the data directory if it is a repo. If it is dirty, preserve unrelated changes.
4. Use JSON tooling for inspection:
   ```bash
   keepbook --config ./keepbook.toml list accounts | jq .
   ```

## What To Read

- CLI commands and common recipes: [references/cli.md](references/cli.md)
- Config fields and examples: [references/config.md](references/config.md)
- Storage layout and file safety: [references/storage.md](references/storage.md)

Only load the reference file needed for the task.

## Operating Rules

- Never hardcode credentials in config or data files. Use `pass find <keyword>` and runtime env vars or supported credential indirection.
- Treat `transactions.jsonl`, `balances.jsonl`, and `transaction_annotations.jsonl` as append-oriented logs. Use `keepbook set transaction` for annotations and `keepbook set balance` for balance snapshots.
- Run `keepbook sync symlinks` after changes that affect connection/account names, membership, or directory links.
- Run `keepbook sync recompact` only when the user wants compaction/deduplication; it rewrites JSONL files.
- For read-only reporting, prefer `portfolio snapshot --offline`, `portfolio history`, `spending`, `list balances`, and `list transactions`.
- For network operations, be explicit about scope: `sync connection`, `sync all --if-stale`, `sync prices account`, or `market-data fetch`.

## Data Directory AGENTS.md

The data-directory AGENTS.md template lives at `agents/keepbook-data/AGENTS.md` in the keepbook repo. Install it with:

```bash
agents/keepbook-data/install --data-dir /path/to/data-dir
```

Default behavior copies files. Use `--mode symlink` only for machine-local directories where links back to this checkout are acceptable.
