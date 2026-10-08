# Claude Design sync notes

Keepbook's UI is Rust/Dioxus. The synced design system is the React mirror
package in `crates/keepbook-dioxus/design/` (`keepbook-design`, `window.Keepbook`).
Each mirror emits the same markup and classes as its Dioxus component and
bundles the app's real `assets/styles.css` and `assets/fonts/inter.css`.

## Building

- Package build: `cd crates/keepbook-dioxus/design && npm ci && npm run build`.
  - esbuild writes `dist/index.js`, `dist/index.css` (the bundled app stylesheet and Inter rule), and `dist/InterVariable.woff2`.
  - `tsc` writes the `.d.ts` tree.
- Converter args:
  - `--node-modules crates/keepbook-dioxus/design/node_modules`
  - `--entry crates/keepbook-dioxus/design/dist/index.js`
  - `cssEntry` is `dist/index.css`: it must stay inside the package dir.
- Render check on NixOS: Playwright's downloaded Chromium can't launch here.
  - `.ds-sync` pins `playwright@1.58.2`, which matches the cached chromium-1208.
  - Launch through a Nix-built headless shell with `DS_CHROMIUM_PATH`. Get the store path from `cat $(which playwright-cli)` (`PLAYWRIGHT_BROWSERS_PATH`), then use `.../chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell`.
- Guidelines come from `docs/design-system/*.md` through `guidelinesGlob`.
- If the render check fails with "Target page, context or browser has been closed" on a run of components in a row, the headless shell crashed. Re-run; it isn't a component bug.

## Previews

- Wide components use `cardMode: column`. At card widths below 1200px, `DataTable`, `TreeGroup` and `SummaryGrid` render their compact or stacked layout. That's the app's real responsive behavior, not a bug.
- `Modal` renders a `position: fixed` backdrop. The preview wraps it in a transformed, fixed-height box so the dialog stays inside the card.
- The `SplitButton` menu opens toward the left, as in the right-aligned toolbar. Its open-menu preview right-aligns the button.
- Every preview has a `Dark` cell wrapped in `Theme`. The single-story cards (`Modal`, `SplitButton`) show both themes inside their primary story. `AppShell`'s dark version is its own `Dark` story.

## Known render warns

- None at the last sync.

## Re-sync risks

- The mirrors are hand-written copies of the Dioxus markup.
  - The `design_mirrors_only_use_classes_the_app_defines` unit test checks class names only, not structure or props.
  - Review mirrors whenever `views/shared.rs` or a view's markup changes.
- Some data is copied by hand from Rust and can silently drift:
  - Chart geometry and copy: `NetWorthChart` (`views/charts/net_worth.rs`), `SpendingChart` (`views/spending/over_time.rs`), `SpendingBreakdown` (`views/spending/pie.rs`, `logic/spending.rs`).
  - Money formatting in `design/src/charts/money.ts` (`logic/format.rs`).
  - The `renderIcon` paths (`ButtonIcon` in `views/shared.rs`).
  - `NAV_ITEMS` (`ActiveView` in `views.rs`).
- The Inter version is 4.1, from the official release zip. Updating it means replacing `assets/fonts/InterVariable.woff2`.
