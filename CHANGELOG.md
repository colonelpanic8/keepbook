# Changelog

## 0.4.16 - 2026-06-18

### Fixed

- Made `[git].ssh_key_path` the canonical SSH identity for all libgit2 fetch,
  pull, push, and auto-push operations.
- Updated Dioxus Git settings to persist selected SSH keys into the shared git
  configuration instead of a Git Sync-only path.

## 0.4.15 - 2026-06-15

### Added

- Added exact and close string-match spending rankings to the Dioxus spending view, backed by headless spending report grouping.

## 0.4.14 - 2026-06-13

### Fixed

- Set the Linux desktop window class before GTK initialization so taskbars can match the Keepbook icon.

## 0.4.13 - 2026-06-13

### Fixed

- Added app-id desktop metadata and icon aliases to Dioxus Linux release bundles.

## 0.4.12 - 2026-06-12

### Fixed

- Wrapped Linux keepbook package executables with the OpenSSL runtime library path.

## 0.4.11 - 2026-06-12

### Fixed

- Fixed the Nix flake package version and OpenSSL inputs for clean keepbook builds from downstream flakes.
- Simplified Marketstack single-date close fetching to avoid a Rust 1.95 release-build compiler ICE.

## 0.4.10 - 2026-06-12

### Added

- Added transaction effective dates for assigning transactions to a reporting date without changing the synced transaction date.
- Added Dioxus spending UI controls for saving and clearing transaction effective dates.

## 0.4.4 - 2026-05-24

### Fixed

- Set the Dioxus desktop window and launcher icon identity consistently on Linux.

## 0.4.3 - 2026-05-23

### Fixed

- Enlarged the Keepbook launcher artwork on mobile by removing excess transparent icon padding.

## 0.4.2 - 2026-05-23

### Fixed

- Corrected Android Git clone paths that retained stale macOS Application Support locations.
- Made the Android Git Sync clone location editable in settings.
- Packaged the keepbook launcher icon in Android release builds.

## 0.4.1 - 2026-05-22

### Fixed

- Constrained the Dioxus desktop workspace and spending layout so fullscreen ultrawide windows do not over-expand the UI.

## 0.4.0 - 2026-05-15

### Added

- Added a spending-over-time chart in the Dioxus UI.
- Added manual value assets so non-priced holdings can be included in portfolio valuation.
- Added stacked net worth charts and tooltips for drilling into portfolio composition.
- Added a recurring transaction review UI backed by persisted recurring review state.
- Added recursive tag hierarchy configuration for aliases, parent tags, and rollups.

### Changed

- Replaced legacy transaction categories with tag and subtag based classification.
- Updated spending, transaction rule, batch tagging, TUI, and AI rule flows to use the tag model consistently.
- Improved portfolio, spending, and balance outputs so tag hierarchy and manual value assets are reflected in derived reports.

### Fixed

- Kept host-local Git SSH key paths out of shared `keepbook.toml`.
- Prevented automated data commits from staging `keepbook.toml`, so local machine settings are not pushed with portfolio or sync snapshots.
