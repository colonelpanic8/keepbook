# XDG Data Directory Design

## Overview

Move keepbook's data directory to follow XDG conventions and make it a private GitHub repo.

## Config Resolution

Order when `--config` not specified:
1. `./keepbook.toml` (current directory) - preserves existing behavior
2. `~/.local/share/keepbook/keepbook.toml` (XDG data dir fallback)

## Changes

### Code Changes (src/config.rs)

Add `default_config_path()` function that:
1. Checks `./keepbook.toml` - return if exists
2. Falls back to `~/.local/share/keepbook/keepbook.toml`

Update `src/main.rs` to use this for the `--config` default.

### File Migration

Move from `./data/*` to `~/.local/share/keepbook/`:
```
~/.local/share/keepbook/
├── keepbook.toml        # config with data_dir = "."
├── accounts/
├── connections/
├── prices/
└── price_sources/
```

### GitHub Setup

1. Initialize git in `~/.local/share/keepbook/`
2. Create private repo `keepbook-data`
3. Initial commit and push

## Security Review

- All credentials use `pass` backend (references only, no plaintext)
- Balance data acceptable for private repo
- No gitignore needed
