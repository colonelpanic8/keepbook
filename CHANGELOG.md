# Changelog

## 0.7.4 - 2026-07-15

### Fixed

- Kept net-worth and account charts aligned with the current portfolio snapshot
  when history contains a UTC date that is tomorrow in the local timezone.
- Refreshed account and net-worth data when navigating back to those views or
  reopening the desktop app from the system tray.
- Moved the Git SSH identity path out of repository `keepbook.toml` files and
  into device-local configuration. Saving Git settings now also removes legacy
  `ssh_key_path` entries from the repository config.
- Fixed desktop Git SSH pushes when an available SSH agent has no usable
  identities. Git authentication now tries configured and default private keys
  before the agent and advances to the next credential after a rejected key.
- Moved the Git sync action to the end of the Accounts view controls.

## 0.7.3 - 2026-07-12

### Fixed

- Made spending-chart tooltips expand to fit their longest line and reposition
  within the chart so text remains inside the tooltip at either edge.

## 0.7.2 - 2026-07-12

### Fixed

- Split multi-tagged transaction amounts across distinct categories at currency
  precision, keeping spending breakdowns additive to their period totals.
- Updated spending-chart segment tooltips to show both the hovered category
  amount and the total for its time bucket.

### Changed

- Kept loaded UI content available during background operations while showing
  nearby progress, success, and failure status.

## 0.7.1 - 2026-07-12

### Fixed

- Replaced bright spending-chart dividers with subtle dark-theme separators.
- Removed stacked-area outlines that rendered as vertical artifacts in the net
  worth breakdown chart.

### Changed

- Renamed the misleading Contributions view to Net Worth Breakdown.

## 0.7.0 - 2026-07-11

### Added

- Added a Settings toggle that starts the desktop app hidden in the system tray.
  The preference is saved to `[tray].start_minimized` and takes effect on the
  next launch.

## 0.6.0 - 2026-07-10

### Added

- Added recurring-cost estimates for the fitted interval, typical charge, and
  annualized cost to the recurring transaction API and Dioxus app.
- Added annual-cost and confidence sorting to the Dioxus recurring-cost view,
  with highest annual cost shown first by default.
- Made the Dioxus web API origin configurable at build time with
  `KEEPBOOK_API_BASE` for remote and Tailscale previews.

### Changed

- Reworked recurring-cost detection to keep only active outflows with stable
  amounts and predictable weekly-through-yearly schedules. Irregular purchases,
  income, expired patterns, and two-occurrence coincidences are excluded.
- Recurring estimates now use only the latest uninterrupted schedule run so
  stale price history does not distort the current cost projection.
- Redesigned recurring cards to foreground interval, per-charge cost, observed
  range, annual cost, next expected date, and supporting transactions.

## 0.5.10 - 2026-07-07

### Fixed

- Fixed Android/mobile Git SSH auth during auto-push by loading configured and
  default private key files into memory before creating libgit2 credentials.
  This avoids libssh2 private-key path handling failures during price refresh
  auto-commit pushes.

## 0.5.9 - 2026-07-07

### Fixed

- Fixed mobile Git auto-commit pushes after price refresh when `[git].ssh_key_path`
  points at a stale or desktop-only private key path. Git credential setup now
  ignores missing key files and falls back to the app-private
  `keepbook_sync_key` identity.

## 0.5.8 - 2026-07-07

### Added

- Clicking a segment of a bar in the spending view now focuses that category
  within that period: the transaction list and totals are limited to the
  selected category and time bucket. A "Focused" chip (and re-clicking the same
  segment) clears the focus.

## 0.5.7 - 2026-07-07

### Fixed

- The net worth chart's last data point now matches the total on the accounts
  page. History requests no longer send a local-timezone end date, which had
  trimmed freshly synced points stamped with the next day's UTC date off the
  end of the chart.

### Added

- Added a "Resync data" action to the accounts view that reloads all data from
  disk (via a new `POST /api/reload` endpoint / native reload) without
  restarting the app.
- Charts now refetch whenever data is refreshed (pull-to-refresh, price
  refresh, or tray sync), so graphs no longer show stale pre-refresh data.
- The CLI honors `KEEPBOOK_DISABLE_AUTO_COMMIT` to suppress git auto-commit
  even when it is enabled in config.

## 0.5.6 - 2026-07-06

### Changed

- Show the keepbook logo next to the title in the Dioxus app's navigation
  header so the brand mark is visible in-app.
- Reduced the Android adaptive launcher icon's foreground from 66% to 55% of
  the canvas so it no longer feels oversized inside launcher masks.

## 0.5.5 - 2026-07-06

### Fixed

- Made selected text (such as prices-sync error messages) copyable in the
  Dioxus desktop app. Release builds suppressed the WebKit context menu, so
  text could be highlighted but the "Copy" option never appeared; the native
  menu is now restored whenever there is an active text selection or editable
  field.

## 0.5.4 - 2026-07-05

### Fixed

- Fixed Android Git push operations failing with libgit2 certificate error -17
  by accepting host certificate callbacks in the Android Git path.

## 0.5.3 - 2026-07-04

### Changed

- Reduced the generated Android launcher icon scale so the adaptive icon uses
  the standard safe-zone size and the legacy square icon has more padding.

## 0.5.2 - 2026-07-02

### Added

- Made the category subsections of the spending "Over Time" bar chart clickable
  in the Dioxus app. Clicking a segment pins the tooltip to show that single
  category's amount and transaction count for that period; clicking it again
  restores the period totals.

### Changed

- Reworked the generated Android launcher icon as an adaptive icon so the logo
  fills more of the masked tile (appears larger) and rendered its background
  with the Keepbook accent green (`#638A68`).

## 0.5.1 - 2026-07-02

### Changed

- Made generated Android launcher icons slightly larger and rendered their
  background with the Keepbook foreground green.

## 0.5.0 - 2026-06-23

### Changed

- Released the current Keepbook application line as minor version 0.5.0.

## 0.4.19 - 2026-06-19

### Changed

- Rendered Android launcher icons on a black background with slightly larger
  Keepbook artwork.

## 0.4.18 - 2026-06-19

### Fixed

- Pointed Android Git SSH known-hosts lookup at app-private storage so mobile
  Git sync can initialize libgit2 without a conventional user home directory.

## 0.4.17 - 2026-06-18

### Added

- Added application version and git commit metadata to the Dioxus settings view.

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
