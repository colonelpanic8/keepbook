# Keepbook Design Tokens

Semantic design tokens for all keepbook UI surfaces. Components reference
**roles** (`var(--color-primary)`), never literal colors. A theme is a single
CSS block that assigns values to roles, so trying on a new look means adding a
`[data-theme="…"]` block — zero component changes.

## Structure

All tokens live at the top of `crates/keepbook-dioxus/assets/styles.css`:

```css
:root,
:root[data-theme="fern"] {
  /* default theme — the current keepbook look */
}

:root[data-theme="dark"] {
  /* overrides only */
}
```

Rules:

- No hex/rgb literals anywhere in stylesheets or rsx outside theme blocks.
- Rust code never hard-codes colors; chart palettes return `var(--series-N)`
  strings. SVG **presentation attributes** (`fill="…"`, `stroke="…"`) do not
  resolve `var()` — set them via `style:` properties instead.
- Themes override the semantic tier directly. Add new roles sparingly; prefer
  reusing an existing role over minting a near-duplicate.

## Color roles — default theme ("fern")

### Surfaces

| Token | Value | Use |
|---|---|---|
| `--color-bg` | `#f4f7f6` | app background |
| `--color-surface` | `#ffffff` | cards, panels, inputs, modals, nav |
| `--color-surface-subtle` | `#f7faf9` | table heads, code chips, tinted panels, empty states |
| `--color-surface-inset` | `#edf4f1` | secondary-button bg, icon-button hover, selected rows |
| `--color-surface-hover` | `#f2f7f5` | table-row hover |
| `--color-surface-muted` | `#f5f6f6` | ignored/disabled rows |
| `--color-surface-warm` | `#fbf8f5` | virtual-group headers |
| `--color-surface-warm-faint` | `#fffdfb` | virtual account rows |

### Text

| Token | Value | Use |
|---|---|---|
| `--color-text` | `#17202a` | primary text, headings, values |
| `--color-text-secondary` | `#31424e` | nav items, code, legends, secondary copy |
| `--color-text-muted` | `#62727f` | labels, captions, `small` |
| `--color-text-faint` | `#7b8790` | de-emphasized meta, disabled-ish text |
| `--color-on-emphasis` | `#ffffff` | text on primary/tooltip/dark fills |

### Borders

| Token | Value | Use |
|---|---|---|
| `--color-border` | `#d7dfdd` | card/panel/nav borders |
| `--color-border-muted` | `#e1e8e6` | lighter card & table borders, panel-header rule |
| `--color-divider` | `#eef2f1` | hairline row/section dividers |
| `--color-border-strong` | `#c8d4d0` | chart axis, dashed empty-state borders |
| `--color-border-input` | `#cfdad7` | inputs and buttons |
| `--color-border-hover` | `#b9cfca` | hovered interactive borders (rows, pills) |

### Brand / primary

| Token | Value | Use |
|---|---|---|
| `--color-primary` | `#1f6f8b` | selected buttons, switches, accent-color, chart line, spinner |
| `--color-primary-strong` | `#174f62` | selected nav text |
| `--color-primary-tint` | `#e8f2f6` | focus-chip bg |
| `--color-accent-tint` | `#e9f1ef` | nav hover/selected bg, tag pills |
| `--color-accent-bg` | `#eef7f4` | info/notice blocks, selected totals |
| `--color-accent-border` | `#cfe2dc` | notice borders |
| `--color-accent-fg` | `#24443d` | notice text, secondary-button text, toggles |
| `--color-accent-muted` | `#7ca99c` | pull-refresh dot border |
| `--color-spinner-track` | `#b8d3cb` | spinner track, pull-refresh dot |

### Status

| Token | Value | Use |
|---|---|---|
| `--color-positive` | `#238a57` | positive deltas |
| `--color-positive-bg` | `#eaf6ef` | success badges (verified, asset status) |
| `--color-positive-fg` | `#2d6b44` | success badge text |
| `--color-negative` | `#bf3d3d` | negative deltas, validation errors |
| `--color-danger-fg` | `#a33a3a` | destructive-button text, dismissed badges |
| `--color-danger-bg` | `#fff4f4` | destructive-button bg |
| `--color-danger-border` | `#e1b5b5` | destructive-button border |
| `--color-warning-bg` | `#f4e7dd` | liability badges |
| `--color-warning-fg` | `#8a4d20` | liability badge text |
| `--color-badge-neutral-bg` | `#eef0f2` | neutral badges (ignored, proposed) |
| `--color-badge-neutral-fg` | `#596670` | neutral badge text, reason chips |
| `--color-badge-neutral-border` | `#d3dbd9` | neutral badge borders |

### Charts & overlays

| Token | Value | Use |
|---|---|---|
| `--chart-grid` | `#e0e8e5` | gridlines |
| `--chart-axis` | `var(--color-border-strong)` | axis lines |
| `--chart-area` | `#dbeee8` | area fill under net-worth line |
| `--chart-line` | `var(--color-primary)` | primary series line |
| `--progress-track` | `#dbe8e4` | indeterminate progress track |
| `--tooltip-bg` | `#17202a` | tooltip fill (used with existing opacity) |
| `--tooltip-fg` | `#ffffff` | tooltip primary text |
| `--tooltip-muted` | `#d8e4e0` | tooltip secondary text |
| `--tooltip-negative` | `#ffd2ca` | negative values in tooltips |
| `--tooltip-border` | `rgba(255, 255, 255, 0.86)` | tooltip outline |
| `--scrim` | `rgba(23, 32, 42, 0.42)` | modal backdrop |
| `--scrim-soft` | `rgba(23, 32, 42, 0.28)` | nav drawer backdrop |
| `--swatch-ring` | `rgb(23 32 42 / 18%)` | legend/tag swatch outline |
| `--shadow-modal` | `0 18px 48px rgba(23, 32, 42, 0.22)` | modal dialogs |
| `--shadow-drawer` | `18px 0 34px rgba(23, 32, 42, 0.18)` | open mobile nav |
| `--shadow-thumb` | `0 1px 3px rgb(23 32 42 / 18%)` | switch thumb |

### Categorical series

Stacked/net-worth series (`--series-1` … `--series-18`), from
`stacked_chart_color`:

`#1f6f8b #c2573f #4f7f39 #7b5cb8 #b7831f #2a9187 #a83f6f #5867c2 #7a6a2f
#3d7898 #d06a2f #5f8c54 #8f4fb0 #c14d57 #2f8fbd #9a7331 #516fb0 #6f7d35`

Spending-tag palette (`--cat-1` … `--cat-10`), from `SPENDING_TAG_COLORS`:

`#1f6f8b #238a57 #8a5cf6 #bf6b21 #b83280 #52677a #2f9e9e #9b6a28 #6f7d1f
#bf3d3d`

### Consolidations (deliberate near-duplicate merges)

These map multiple old literals onto one role; the delta is visually
imperceptible and the divergence was accidental:

- `#f8faf9` → `--color-surface-subtle`
- `#2a3640`, `#43515b` → `--color-text-secondary`
- `#71808b`, `#69747d` → `--color-text-faint`
- `#e4e9e7` → `--color-border-muted`
- `#cbd7d3` → `--color-border-strong`
- `#cddbd7`, `#d8e1de` → `--color-border-input`
- `#9fbab3` → `--color-border-hover`
- `#e9f3ef` → `--color-accent-tint`
- `#e8f1ef` → `--color-positive-bg`, `#286153` → `--color-positive-fg`
- `#b83232` → `--color-negative`
- `#8b2f2f` → `--color-danger-fg`, `#f8eeee` → `--color-danger-bg`,
  `#e4c6c6` → `--color-danger-border`
- `#eef2f6`, `#e6eaea` → `--color-badge-neutral-bg`;
  `#52616f`, `#536760` → `--color-badge-neutral-fg`
- `#172a3a` → `--color-text` (pinned bar stroke)
- ignored-badge text `#62727f` stays `--color-text-muted`
- the two danger button variants (`.danger-button`, `.danger`) unify onto the
  danger tokens above

## Non-color tokens

### Radii

`--radius-xs: 3px` (swatches) · `--radius-sm: 6px` (buttons, inputs, chips) ·
`--radius-md: 8px` (cards, panels, modals) · `--radius-pill: 999px`

### Typography

```
--font-sans: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
--font-mono: ui-monospace, "SFMono-Regular", "SF Mono", Consolas, "Liberation Mono", monospace;
```

Size scale (map old rem sizes to the nearest token; consolidations noted):

| Token | Value | Absorbs |
|---|---|---|
| `--fs-micro` | `0.7rem` | 0.68 |
| `--fs-caption` | `0.75rem` | 0.72, 0.76 |
| `--fs-label` | `0.78rem` | — |
| `--fs-small` | `0.82rem` | — |
| `--fs-control` | `0.85rem` | 0.84, 0.86 |
| `--fs-body` | `0.9rem` | — |
| `--fs-title-sm` | `0.95rem` | — |
| `--fs-title` | `1rem` | — |
| `--fs-lead` | `1.1rem` | 1.15 |
| `--fs-heading` | `1.2rem` | — |
| `--fs-stat-sm` | `1.3rem` | — |
| `--fs-stat` | `1.6rem` | 1.55 |

SVG `px` font sizes (11/12/13/18px in chart text) stay literal — they scale
with the viewBox, not the document.

Weights: `--fw-medium: 600`, `--fw-semibold: 700` (absorbs 750),
`--fw-bold: 800`, `--fw-black: 900`.

### Spacing

Value-named px tokens (theme-swappable for density without changing today's
look): `--sp-2: 2px`, `--sp-4: 4px`, `--sp-6: 6px`, `--sp-8: 8px`,
`--sp-10: 10px`, `--sp-12: 12px`, `--sp-14: 14px`, `--sp-16: 16px`,
`--sp-20: 20px`, `--sp-24: 24px`.

Apply to single-value `gap`/`padding`/`margin` declarations that match a token
exactly. Compound shorthands (`padding: 8px 10px`) and odd one-off values
(3/5/7/9px, layout dimensions, breakpoints) stay literal — tokenizing those is
churn without theming payoff.

### Compact navigation

`--compact-header-control-height: 38px` keeps the compact navigation toggle,
repository selector, and logo aligned to one shared height.
`--compact-header-padding-block: 10px` and
`--compact-header-padding-inline: 14px` define the compact bar's sole padding
geometry. Opening navigation attaches a drawer beneath the bar instead of
overriding that geometry. Components must reference these tokens rather than
repeating their values.
