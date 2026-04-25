# keepbook constellation

## Scope
- Use this guide for requests involving keepbook data directories, keepbook CLI usage, or keepbook repository work.
- For data-directory operation rather than code changes, use the `keepbook-data` skill.

## Related packages/projects
- `keepbook`: local-first personal finance toolkit and CLI.
- `keepbook.toml`: keepbook config file.
- keepbook data directories containing `connections/`, `accounts/`, `price_sources/`, `assets/`, `prices/`, or `fx/`.

## Symlink targets
- `./project-links/keepbook` -> primary keepbook repo.

## Discovery hints
- Start from `~/Projects/keepbook` for source.
- Default keepbook config lookup is `./keepbook.toml`, then `~/.local/share/keepbook/keepbook.toml`.
- Use `keepbook --config <path>` when operating on a specific data/config directory.

## Read-first docs
- For source-code changes: `./project-links/keepbook/AGENTS.md`
- For data-directory operation: `./skills/keepbook-data/SKILL.md`
