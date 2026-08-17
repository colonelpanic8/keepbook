# Interaction states

Keepbook stays usable while background work runs. An operation should explain
what is happening without replacing already-loaded content or making the whole
app appear frozen.

## In-flight operations

- Start user-triggered work asynchronously. Never run network, disk, sync, Git,
  market-data, or AI work on the UI thread.
- Show a nearby `OperationStatus` as soon as work starts. Use a concrete present
  participle such as “Refreshing prices…” or “Saving tags…”, plus an
  indeterminate spinner when exact progress is unavailable.
- Put `aria-busy="true"` on the active status and control. Announce status text
  with a polite live region; do not repeatedly announce animation frames.
- Keep navigation, already-loaded content, scrolling, filtering, and unrelated
  actions available. Disable only the initiating control and actions that would
  conflict with the same mutation.
- Preserve the current content while revalidating it. Initial loads may use a
  local skeleton or activity panel; background refreshes must not swap the page
  for a blank screen or full-screen spinner.
- Prevent duplicate submission while an operation is active. The initiating
  button shows a compact spinner and an active verb when space permits.

## Completion and recovery

- Replace progress text with a concise success or failure result. Do not leave a
  spinner running after the future resolves.
- A failed load offers an in-context retry. A failed mutation restores its
  controls and keeps the user's inputs whenever retrying is safe.
- Long operations that support safe cancellation (currently Git clone/sync)
  expose Cancel. Cancellation must be cooperative and must restore controls
  whether it succeeds, fails, or is canceled.
- If an operation has no safe cancellation mechanism, users must still be able
  to navigate elsewhere. Do not imply that closing a view cancels server work.

## Repository switching

- The navigation repository selector reflects the app-wide active repository.
  Repositories without a completed clone are visible but cannot be selected.
- Repository declarations in the XDG `app.toml` manifest are read-only and
  display as managed. The active selection and repositories added from
  Settings live in device-local `device.toml`; Settings never rewrites the
  manifest and does not offer Remove for a managed entry.
- Switching repositories is asynchronous. Keep the current repository visible
  until the replacement config and storage load successfully, then refresh all
  repository-derived views together.
- Adding or removing a repository changes only device-local state. Removing an
  entry never deletes its local files. Git clone and sync use the remote and
  branch stored with that registry entry and retain the standard cancel flow.
- Declarative setup is an explicit CLI operation. Normal app startup remains
  network-free and shows un-cloned manifest entries as unavailable.

## Shared components

- `BackendActivity` is for initial, local data-region loading only.
- `GraphLoadingPanel` is the chart-specific initial/loading treatment.
- `OperationStatus` is the standard persistent feedback for user-triggered work.
- `ControlButton`'s `busy` state adds the compact spinner, `aria-busy`, and
  duplicate-submission protection.

## Chart hover details

- Hovering a stacked spending segment identifies the category and shows both
  that segment's amount and the total for its day, week, month, quarter, or
  year. The tooltip background expands for its longest line and shifts within
  the chart bounds rather than clipping text at either edge. Hovering empty
  space in the bucket shows the bucket total alone.
- Clicking a segment pins the same category-and-bucket detail while the chart
  and transaction list focus on that selection.

## Assets table

- The Assets view renders one `.data-table.assets-table` row per (asset,
  liability) pair from the portfolio asset breakdown: Asset, Amount, Price,
  Value in the reporting currency, price freshness, 1D/1W/1M/1Y trailing
  changes, and finally the lower-priority amount checked/changed timestamps,
  plus a trailing expander column. Data loads through the standard
  refresh-epoch/`use_resource` flow with `BackendActivity` for the initial load
  and `InlineStatus` for failures.
- Column headers reuse the shared `sort-header-button`/`sort-arrow` pattern
  from the transaction table (Price is display-only). At the shared mobile
  breakpoint, headers become a dedicated `asset-mobile-sort` field and
  direction control while rows become labeled cards; the mobile field exposes
  every desktop sort in display-priority order: Asset, Amount, Value, Price
  updated, 1D, 1W, 1M, 1Y, Amount checked, and Amount changed. Sorting is
  client-side; the default is Value descending by **absolute** value, name
  sorts default ascending, and rows without a metric (unpriced values, absent
  timestamps or change periods) always sort last in either direction.
- Mobile asset cards foreground the asset identity and reporting-currency
  value together on one scan line, pair Amount with Price, group the four
  trailing changes, and leave freshness metadata out of the collapsed scan
  path. The compact phone layout drops the redundant count cards from the
  summary and keeps sorting on one line. Price updated, Amount checked, and
  Amount changed appear after performance when the row is expanded, with the
  amount timestamps last in the quietest labeled metadata group. Expanded
  account holdings use the same labeled-card treatment instead of relying on
  hidden headers.
- Clicking a row, or its `transaction-expand-toggle` chevron, toggles
  `.asset-holding-row` sub-rows on the `--color-surface-subtle` surface
  listing each contributing account's amount, base-currency value, and
  balance date. Expansion is keyed by asset id plus the liability flag, so an
  asset's debt row expands independently of its asset row.
- Rows aggregating negative holdings keep their own row (no netting) and get
  the existing `status liability-status` badge, matching the accounts view's
  badge treatment.
- Change cells show the signed percentage colored with `change-positive` /
  `change-negative`; exactly zero stays neutral (no class). The signed
  absolute change is exposed as the cell tooltip. When a period exists but
  has no comparable past value (a new position), the cell shows the signed
  absolute change instead; a period with no data shows an em dash.

Status treatments use semantic roles from `styles.css`: accent colors for
in-flight activity, neutral surfaces for settled messages, and the shared
spinner/progress tokens. Add new state colors only through the token document.
