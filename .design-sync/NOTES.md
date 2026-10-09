# Claude Design sync notes

Keepbook's UI is Rust/Dioxus. The synced design system is the React mirror
package in `crates/keepbook-dioxus/design/` (`keepbook-design`, `window.Keepbook`).
Each mirror in `design/src/<group>/<Name>.tsx` emits the same markup as its
Dioxus component in `src/components/<group>/<name>.rs`, checked by the parity
snapshots in `design/parity/` (`cargo test -p keepbook-dioxus` and `npm test`
in `design/`). The package bundles the app's real `assets/styles.css` and
`assets/fonts/inter.css`.

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
- `StatusStack` is `position: fixed` to the viewport's bottom-right, so its preview uses the same transformed box.
- The `SplitButton` menu opens toward the left, as in the right-aligned toolbar. Its open-menu preview right-aligns the button.
- Previews render through the `PreviewTheme` provider, which applies the theme saved by the `ThemePicker` card (or a `?theme=` URL parameter), so one pick re-themes every card. The `Theme` card shows every theme side by side.

## Known render warns

- None at the last sync.

## Re-sync risks

- Markup parity covers each case in `tests/unit/parity_tests.rs`, not every prop combination. Add a case on both sides when a component gains a variant.
- `design/parity/cases.tsx` mirrors the Rust case inputs by hand. A case that passes with different inputs proves nothing, so keep the two lists identical.
- Chart props differ by design: the Rust charts take app outputs (`SpendingOutput`, `HistoryChangeSummary`), while the mirrors take the derived values (buckets, totals, change text) a designer can type. The geometry and copy are ported, and the parity cases pin them.
- Money formatting in `design/src/charts/money.ts` copies `logic/format.rs`.
- `PreviewTheme` and `Theme` are design-only; `componentSrcMap` hides `PreviewTheme` from the component list.
- The design bundle imports `assets/theme.js` and the Rust-generated `design/parity/dynamic-theme.css`. Designs can't regenerate Dynamic for another seed; picking one in the design `ThemePicker` keeps the sample.
- The Inter version is 4.1, from the official release zip. Updating it means replacing `assets/fonts/InterVariable.woff2`.
