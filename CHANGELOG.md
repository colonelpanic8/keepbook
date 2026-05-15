# Changelog

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
