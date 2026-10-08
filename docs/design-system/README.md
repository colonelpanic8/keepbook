# Keepbook design system

Keepbook is a local-first personal finance app: net worth, accounts, assets,
spending, and recurring costs, computed from plain files on the user's
machine. The UI is a dense, quiet tool for reading numbers. It runs as a
Dioxus app on desktop, web, and Android, all sharing one stylesheet.

The system is deliberately small. There is one token per role, one class
per component, and two themes that differ only in token values. Before
adding anything, reuse what exists. If something new is genuinely needed,
replace an old pattern rather than adding a parallel one.

## Where things live

| Path | What it is |
|---|---|
| `crates/keepbook-dioxus/assets/styles.css` | The source of truth: theme tokens and every component rule. |
| `crates/keepbook-dioxus/assets/fonts/` | Inter (variable WOFF2, OFL) and its shared `@font-face` rule. |
| `crates/keepbook-dioxus/src/views/shared.rs` | The Dioxus components. |
| `crates/keepbook-dioxus/design/` | React mirrors of those components, synced to Claude Design. |
| `.design-sync/` | Claude Design sync config, notes, and authored preview cards. |
| `interaction-states.md` | Loading, progress, failure, and cancellation behavior. |

Update this README in the same change whenever a pattern is added or changed.
See [Claude Design](#claude-design) for how the mirrors and the synced project
stay current.

Set `data-theme="fern"` (the default) or `data-theme="dark"` on `<html>` or
on any container.

## Content

- Write in sentence case: "Refresh prices", "Net worth breakdown".
- Keep copy terse. A label names the thing; a subtitle adds one fact.
- Report work in progress with a present participle and an ellipsis:
  "Refreshing prices…". Report results in the past tense with a count where
  there is one: "Refreshed 14 prices."
- Format money with the currency symbol, thousands separators, and two
  decimals ("-$1,436.20"). Put the sign before the symbol.
- Write dates as ISO `YYYY-MM-DD`. Write ranges as `start → end` or
  `start to end`.
- Show a missing value as an em dash (—), never as blank, `0`, or "N/A".
- Show changes as signed percentages ("+2.4%") colored with
  `.change-positive` / `.change-negative`. An exact zero stays neutral.

## Visual foundations

### Color

Components reference roles, never literal colors. Each theme is a single
token block. To try a new look, add a `[data-theme="…"]` block; components
don't change. Hex values live in the theme blocks at the top of `styles.css`.

| Role | Tokens | Use |
|---|---|---|
| Surfaces | `--color-bg`, `--color-surface`, `--color-surface-subtle`, `--color-surface-inset` | Page; panels and inputs; table heads and quiet cards; hover, selected rows, and secondary buttons |
| Text | `--color-text`, `--color-text-muted`, `--color-on-primary` | Content; labels and meta; text on a primary fill |
| Lines | `--color-divider`, `--color-border`, `--color-border-strong` | Hairlines and quiet card edges; controls and panels; hover borders, axes, and dashed placeholders |
| Brand | `--color-primary`, `--color-accent-bg`, `--color-accent-border`, `--color-accent-fg` | Selection, primary actions, and chart lines; the tinted "active / in progress" treatment |
| Status | `--color-positive(-bg)`, `--color-negative(-bg)`, `--color-warning(-bg)` | Gains and success; losses, errors, and destructive actions; liabilities |
| Overlays | `--color-tooltip-bg`, `--color-tooltip-fg`, `--scrim`, `--shadow-sm`, `--shadow-lg` | Chart tooltips; modal and drawer backdrop; switch thumb and selected segment; modals and the drawer |
| Data | `--series-1` … `--series-10` | Chart series and spending tags, assigned by position and wrapping |

Rules:

- Keep hex and rgb literals out of everything except the theme blocks. Rust
  code gets data colors from `series_color(index)`, which returns
  `var(--series-N)`.
- Color SVG through CSS classes or `style:` properties rather than
  presentation attributes (`fill="…"`), so chart colors go through the same
  cascade and theme scoping as everything else.
- Express transparency with `fill-opacity` / `stroke-opacity` on an existing
  role, as the chart area and drag selection do. Don't add an rgba token.
- Every text color must reach WCAG AA (4.5:1) on the surfaces it sits on.

### Type

The font is Inter, shipped with the app as a variable WOFF2, with
`--font-mono` for paths and hashes. There are five sizes and two weights:

| Token | Size | Use |
|---|---|---|
| `--fs-display` | 1.5rem | Headline values (net worth, totals) |
| `--fs-title` | 1.125rem | Card values, the app name |
| `--fs-body` | 1rem | Body copy, table cells, panel titles |
| `--fs-small` | 0.8125rem | Buttons, notices, secondary text |
| `--fs-caption` | 0.75rem | Badges, timestamps, chart axes, overline labels |

`--fw-medium` (600) is for controls, navigation, and labels. `--fw-bold`
(700, the same as `strong`) is for values, headings, and badges. Use
`.control-label` for the system's only uppercase style, the overline. It
also styles table heads.

Use `font-variant-numeric: tabular-nums` where digits need to line up in a
column.

### Space and shape

- Spacing comes from `--sp-2`, `--sp-4`, `--sp-8`, `--sp-12`, `--sp-16`, and
  `--sp-24`.
  - Panels and cards are padded `--sp-16`, and so are the gaps between them.
  - Rows inside a panel are spaced `--sp-12`, and controls in a group
    `--sp-8`.
- Radii:
  - `--radius-sm` (6px) for controls.
  - `--radius-md` (8px) for panels, cards, and tables.
  - `--radius-pill` for badges, chips, switches, and swatches.
- Control heights:
  - `--control-height` (36px) for buttons, inputs, and the segmented track.
  - `--control-height-sm` (28px) for compact buttons, segments, pills, and
    icon buttons.
- Separate surfaces with 1px borders. Shadows are reserved for elements that
  float: `--shadow-lg` for modals and the drawer, `--shadow-sm` for the
  switch thumb and the selected segment.

### Motion

State changes ease over 120ms (hover, selection, switches). The navigation
drawer slides over 160ms. Spinners and progress bars are the only looping
motion. `prefers-reduced-motion: reduce` turns transitions off and slows or
stills the loops.

### Iconography

Icons appear only where they speed up recognition of a repeated action, such
as the page toolbar. They are Lucide stroke icons on a 24px grid, drawn
inline at 14px with `stroke="currentColor"`, so they follow the button's text
color, including on a primary fill. Add a new icon as a `ButtonIcon` variant
in `views/shared.rs`; the current set is Refresh, GitBranch, and
ChevronDown. Small affordances are plain glyphs, not icons:

| Glyph | Use |
|---|---|
| `›` | Expand a row, next page |
| `‹` | Previous page |
| `↓` / `↑` | Sort direction |
| `▾` | Disclosure |
| `×` | Clear a filter |
| `+` | Add |

The logo is `assets/keepbook-icon.svg` at the repository root.

## Components

The Rust components in `crates/keepbook-dioxus/src/views/shared.rs` emit this
markup. Use them instead of hand-writing the classes.

| Component | Markup | Rules |
|---|---|---|
| Panel | `section.panel` › `.panel-header` › `.panel-title` (`h2` + subtitle `span`) | The unit of page content. It is a single column that can't exceed its own width. `Panel` |
| Metric | `article.metric` › `.metric-label`, `strong`, `small`, inside `.summary-grid` | Three across, stacking below 1200px. `MetricCard` |
| Button | `.control-button` + `.primary` / `.selected` / `.danger` / `.small` | One `primary` per toolbar. `.selected` is a toggled state, not emphasis. While busy, a spinner replaces the icon and the button is disabled. `ControlButton` |
| Split button | `.split-button` › `.control-button` + `.control-button.split-button-caret`, then `.menu-backdrop` + `.menu` › `button.menu-item` (`strong` + `small`) | A main action plus a chevron that opens its less common variants, each with a one-line description. Escape or a click outside closes the menu. `SplitButton` |
| Icon button | `.icon-button` | A transparent 28px square for a glyph. It needs an `aria-label`. |
| Segmented control | `.segmented-field` › `.control-label` + `.segmented-control` › `.segment` | For mutually exclusive options. Never use a wrapping row of buttons for this. `SegmentedControl` |
| Input | `.control-input` (+ `.small`), `select.control-input`, inside `label.control-field` | Selects draw their own caret so they follow the theme. |
| Checkbox | `label.compact-check` › `input[type=checkbox]` + text | The color comes from `accent-color` set at the root. |
| Switch | `label.switch-control` › `input` + `.switch-track` › `.switch-thumb` | For a setting that takes effect immediately. It sits in a `.setting-row`. |
| Setting row | `.setting-row` › `.setting-copy` (`strong` + `small`) + one control | Add `.setting-row-stacked` when the control is a select or an option group. |
| Badge | `.badge` + `.positive` / `.negative` / `.warning` | A short status. Neutral unless toned. |
| Tag pill | `.tag-pill` (`.readonly`, `.removable`), `.tag-suggestion-pill` | Spending tags. Tags get color from the swatch, not the pill. |
| Filter chip | `.filter-clear-chip` | Exists only while a filter is active. Clicking it clears the filter. |
| Notice | `.notice` (+ `.busy` with `.activity-spinner`) | Feedback for work the user started, placed next to the control that started it. `OperationStatus`, `BackendActivity` |
| Progress | `.activity-spinner` (`.large`, `.control-spinner`), `.indeterminate-progress` | Indeterminate only. |
| Empty / loading | `.chart-empty`, `.chart-loading` (`strong` + `small`) | Holds the footprint of the content it replaces. `GraphLoadingPanel` |
| Data table | `.data-table` › `.table-head` + `.table-row` | Each table sets its columns with `grid-template-columns`. Below 1200px rows become labeled cards. |
| Tree group | `.tree-group` › `.tree-parent` + `.data-table` | Accounts grouped by connection. |
| Modal | `.modal-backdrop` › `.modal-dialog` › `.modal-header`, body, `.modal-actions` | `Modal` |
| Navigation | `.app-nav` › `.nav-header` + `nav` › `.nav-button(.selected)` | A sidebar at wide widths, a sticky header with a drawer below 1200px. |
| Spending over time | `.chart-card.spending-over-time-card` › `.chart-meta`, `svg.spending-bar-chart` of `.spending-bar-segment`s, `.stacked-legend` | Stacked by tag; focusing a tag narrows the bars to it. |
| Spending by tag | `.spending-layout` › `.spending-pie` + `.tag-list` (`.spending-total`, `.tag-row`s) | Donut and tag rows share the series colors by position. |

## Layout

- Breakpoints:
  - 1200px: compact header, card tables, single-column grids.
  - 720px: segmented labels move above their controls.
  - 520px: setting rows and repository rows stack; toolbar controls grow to
    fill the row.
- The workspace is capped at `--workspace-max-width` and centered.
- Page-level actions (refresh, resync, sync) go in a `.page-toolbar` at the
  top of the view, above the metrics and right-aligned, never in a panel
  header. Their `notice` lines render directly beneath it. Below 520px its
  controls grow to fill the row.
- Keep a toolbar to its main actions. A variant of an action or a rarely used
  action goes in a `SplitButton` menu with a one-line description, never in a
  checkbox beside the button or as another button. The Accounts toolbar shows
  only "Git sync" and "Refresh all prices"; "Refresh stale prices" and
  "Resync data" sit behind the chevron.
- Navigation stays visible while the workspace scrolls.
  - At wide widths it is a fixed sidebar.
  - Narrower, it is a compact header stuck to the top. It holds the
    hamburger, then the repository selector, then the logo, and opening the
    drawer leaves it unchanged.
  - The drawer slides in from the left over 160ms while the scrim fades.
    Both stay in the DOM (an `open` class toggles them), and `visibility`
    flips after the slide so the closed drawer leaves the focus order.
  - Clip horizontal overflow on navigation ancestors with `overflow-x: clip`,
    not `hidden`. Otherwise the sticky header stops tracking the viewport.
- Segmented controls publish `--segment-count`, and the track renders exactly
  that many equal columns, so an option can never wrap onto a second line.
  - Labels shrink first (fluid type sized from the track's container width),
    then ellipsize.
  - Inside a setting row the row's copy names the group, so
    `.setting-segmented` hides the field label.
  - Don't put actions such as "Clear" inside an option group. Give them their
    own row.
- Panels are one `minmax(0, 1fr)` column. Machine values shown in full
  (paths, URLs) set `overflow-wrap: anywhere`. Identifiers shown as a token
  (hashes in a repository row) stay on one line and ellipsize.
- A setting row pairs copy with one control. A switch sits beside its copy at
  any width. A select or option group is `.setting-row-stacked` and stacks
  below 520px.
- Empty and loading states keep the footprint of what they replace. Chart
  placeholders use the chart's aspect ratio (720×260, or 720×300 for the
  spending and stacked charts). List and table placeholders use a
  `min-height` instead.

## Charts

- Charts are SVG with a fixed `viewBox` scaled to the panel width.
  - Gridlines use `--color-divider` and the axis `--color-border-strong`.
  - Axis text is `--fs-caption` muted.
  - The line is `--color-primary` over an area of the same color at 14%
    opacity.
- Series and tags take `--series-N` by position. Stacked segments are
  separated by `--color-surface` strokes, so they read correctly in both
  themes.
- Tooltips are dark in both themes (`--color-tooltip-bg`). Secondary lines
  use `--color-tooltip-fg` at 80% opacity. A tooltip expands to fit its
  longest line and shifts within the chart bounds rather than clipping.
- Hovering a stacked spending segment shows the category amount and the
  bucket total. Clicking a segment pins that detail and focuses the
  transaction list. See [interaction states](interaction-states.md) for the
  rest of the hover and loading behavior.

## Claude Design

The design system is synced to Claude Design (claude.ai/design) as the
**Keepbook** project. The Claude Design agent builds with React, so
`crates/keepbook-dioxus/design/` holds a React mirror of each component. Every
mirror emits exactly the markup and classes of its Dioxus counterpart and
uses `styles.css` and `inter.css` unchanged.

- Change a component's markup or props in `views/shared.rs` and update its
  mirror in the same commit, under the same name and prop names.
- A new reusable class-based pattern gets a mirror and a preview card in
  `.design-sync/previews/<Name>.tsx`.
- A keepbook-dioxus unit test fails if a mirror uses a class that neither
  `styles.css` nor the Rust views define.
- After visual changes, re-run `/design-sync` to update the project. Its
  `.design-sync/NOTES.md` records how the sync is built.
