#!/usr/bin/env python3
import re, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
CSS_PATH = ROOT / "crates/keepbook-dioxus/assets/styles.css"
OUT = ROOT / "docs/design-system"
OUT.mkdir(parents=True, exist_ok=True)

CSS = CSS_PATH.read_text()

def block_body(selector_regex):
    """Return the inner body (declarations) of the first CSS rule whose selector matches."""
    m = re.search(selector_regex + r"\s*\{", CSS)
    if not m:
        raise SystemExit("selector not found: " + selector_regex)
    i = m.end()
    depth = 1
    start = i
    while depth:
        c = CSS[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        i += 1
    return CSS[start:i-1].strip("\n")

FERN_BODY = block_body(r":root,\s*:root\[data-theme=\"fern\"\]")
DARK_BODY = block_body(r":root\[data-theme=\"dark\"\]")

# Parse individual token values from the fern block (theme-independent-ish tokens).
def tokens_from(body):
    d = {}
    for name, val in re.findall(r"(--[\w-]+)\s*:\s*([^;]+);", body):
        d[name] = val.strip()
    return d

FERN = tokens_from(FERN_BODY)

def scoped_theme_css():
    return (
        '[data-theme-scope="fern"] {\n' + FERN_BODY + "\n}\n\n"
        '[data-theme-scope="dark"] {\n' + DARK_BODY + "\n}\n"
    )

SCAFFOLD = """
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 24px;
  font-family: var(--font-sans);
  background: var(--color-bg);
  color: var(--color-text);
  line-height: 1.4;
}
.ds-h1 { font-size: var(--fs-heading); font-weight: var(--fw-bold); margin: 0 0 4px; }
.ds-sub { color: var(--color-text-muted); font-size: var(--fs-small); margin: 0 0 20px; max-width: 70ch; }
.ds-sub code { font-family: var(--font-mono); }
.ds-themes { display: flex; gap: 20px; flex-wrap: wrap; align-items: flex-start; }
.ds-theme {
  flex: 1 1 380px;
  min-width: 0;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: 16px;
  background: var(--color-bg);
  color: var(--color-text);
}
.ds-theme-label {
  font-family: var(--font-mono);
  font-size: var(--fs-caption);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--color-text-muted);
  margin-bottom: 14px;
}
.ds-group-title {
  font-size: var(--fs-label);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: var(--fw-bold);
  color: var(--color-text-muted);
  margin: 18px 0 8px;
}
.ds-group-title:first-of-type { margin-top: 4px; }
.ds-swatches { display: grid; grid-template-columns: repeat(auto-fill, minmax(148px, 1fr)); gap: 10px; }
.ds-swatch {
  border: 1px solid var(--color-border-muted);
  border-radius: var(--radius-sm);
  overflow: hidden;
  background: var(--color-surface);
}
.ds-chip {
  height: 46px;
  border-bottom: 1px solid var(--swatch-ring);
  box-shadow: inset 0 0 0 1px var(--swatch-ring-inner);
}
.ds-chip.checker {
  background-image:
    linear-gradient(45deg, #999 25%, transparent 25%),
    linear-gradient(-45deg, #999 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, #999 75%),
    linear-gradient(-45deg, transparent 75%, #999 75%);
  background-size: 12px 12px;
  background-position: 0 0, 0 6px, 6px -6px, -6px 0;
  background-color: #fff;
}
.ds-name {
  font-family: var(--font-mono);
  font-size: var(--fs-micro);
  padding: 6px 7px;
  color: var(--color-text-secondary);
  word-break: break-all;
}
.ds-val {
  font-family: var(--font-mono);
  font-size: var(--fs-micro);
  color: var(--color-text-faint);
  padding: 0 7px 6px;
  word-break: break-all;
}
.ds-row { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; margin-bottom: 14px; }
.ds-col { display: flex; flex-direction: column; gap: 12px; }
.ds-caption { font-family: var(--font-mono); font-size: var(--fs-micro); color: var(--color-text-muted); }
.ds-cell { display: flex; flex-direction: column; gap: 5px; align-items: flex-start; }
"""

def page(marker, title, extra_style, body):
    return (
        f"<!-- {marker} -->\n"
        "<!doctype html>\n"
        '<html lang="en">\n<head>\n'
        '<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
        f"<title>{title}</title>\n"
        "<style>\n" + CSS + "\n</style>\n"
        "<style>\n" + SCAFFOLD + extra_style + "\n</style>\n"
        "</head>\n<body>\n" + body + "\n</body>\n</html>\n"
    )

def themed(scope, label, inner):
    return (
        f'<section class="ds-theme" data-theme-scope="{scope}">\n'
        f'<div class="ds-theme-label">{label}</div>\n'
        f"{inner}\n</section>"
    )

def two_themes(inner_builder):
    return (
        '<div class="ds-themes">\n'
        + themed("fern", 'data-theme="fern" (default)', inner_builder())
        + "\n"
        + themed("dark", 'data-theme="dark"', inner_builder())
        + "\n</div>"
    )

# ---------------------------------------------------------------- colors.html
COLOR_GROUPS = [
    ("Surfaces", ["--color-bg","--color-surface","--color-surface-subtle","--color-surface-inset",
                  "--color-surface-hover","--color-surface-muted","--color-surface-warm","--color-surface-warm-faint"]),
    ("Text", ["--color-text","--color-text-secondary","--color-text-muted","--color-text-faint","--color-on-emphasis"]),
    ("Borders", ["--color-border","--color-border-muted","--color-divider","--color-border-strong",
                 "--color-border-input","--color-border-hover"]),
    ("Brand / primary", ["--color-primary","--color-primary-strong","--color-primary-tint","--color-accent-tint",
                         "--color-accent-bg","--color-accent-border","--color-accent-fg","--color-accent-muted","--color-spinner-track"]),
    ("Status", ["--color-positive","--color-positive-bg","--color-positive-fg","--color-negative","--color-danger-fg",
                "--color-danger-bg","--color-danger-border","--color-warning-bg","--color-warning-fg",
                "--color-badge-neutral-bg","--color-badge-neutral-fg","--color-badge-neutral-border"]),
    ("Charts & overlays", ["--chart-grid","--chart-axis","--chart-area","--chart-line","--chart-selection-fill",
                           "--chart-selection-stroke","--progress-track","--tooltip-bg","--tooltip-fg","--tooltip-muted",
                           "--tooltip-negative","--tooltip-border","--scrim","--scrim-soft","--swatch-ring",
                           "--swatch-ring-inner","--shadow-modal","--shadow-drawer","--shadow-thumb"]),
]
TRANSLUCENT = {"--chart-selection-fill","--chart-selection-stroke","--tooltip-border","--scrim","--scrim-soft",
               "--swatch-ring","--swatch-ring-inner","--shadow-modal","--shadow-drawer","--shadow-thumb"}

def colors_inner():
    out = []
    for gname, toks in COLOR_GROUPS:
        out.append(f'<div class="ds-group-title">{gname}</div>')
        out.append('<div class="ds-swatches">')
        for t in toks:
            checker = " checker" if t in TRANSLUCENT else ""
            if t.startswith("--shadow"):
                chipstyle = f"box-shadow: var({t}); background: var(--color-surface);"
            else:
                chipstyle = f"background: var({t});"
            out.append(
                '<div class="ds-swatch">'
                f'<div class="ds-chip{checker}" style="{chipstyle}"></div>'
                f'<div class="ds-name">{t}</div></div>'
            )
        out.append("</div>")
    return "\n".join(out)

colors_body = (
    '<div class="ds-h1">Color roles</div>\n'
    '<p class="ds-sub">Every semantic color token, grouped as in <code>docs/design-tokens.md</code>. '
    "Rendered under both theme scopes side by side. Translucent tokens (overlays, rings, shadows) sit on a "
    "checkerboard so their alpha reads.</p>\n"
    + two_themes(colors_inner)
)
(OUT / "colors.html").write_text(page(
    '@dsCard group="Foundations" name="Color roles"',
    "Color roles — keepbook design tokens",
    "", colors_body))

# ---------------------------------------------------------------- typography.html
FS_ORDER = ["--fs-micro","--fs-caption","--fs-label","--fs-small","--fs-control","--fs-body",
            "--fs-title-sm","--fs-title","--fs-lead","--fs-heading","--fs-stat-sm","--fs-stat"]
FW_ORDER = ["--fw-medium","--fw-semibold","--fw-bold","--fw-black"]

typ_rows = []
for t in FS_ORDER:
    v = FERN[t]
    typ_rows.append(
        '<div class="ds-cell" style="margin-bottom:16px;">'
        f'<div style="font-size: var({t}); font-weight: var(--fw-semibold);">Portfolio net worth $128,400</div>'
        f'<div class="ds-caption">{t} &middot; {v}</div></div>'
    )
fw_rows = []
for t in FW_ORDER:
    v = FERN[t]
    fw_rows.append(
        '<div class="ds-cell" style="margin-bottom:12px;">'
        f'<div style="font-size: var(--fs-lead); font-weight: var({t});">The quick brown fox</div>'
        f'<div class="ds-caption">{t} &middot; {v}</div></div>'
    )
typo_body = (
    '<div class="ds-h1">Type scale</div>\n'
    '<p class="ds-sub">Font-size and weight tokens rendered at their real values, plus the two font stacks. '
    "Values are theme-independent.</p>\n"
    '<div class="ds-group-title">Font stacks</div>\n'
    '<div class="ds-col" style="margin-bottom:8px;">'
    '<div class="ds-cell"><div style="font-family: var(--font-sans); font-size: var(--fs-lead);">'
    "Inter / system sans — 1234567890</div>"
    f'<div class="ds-caption">--font-sans &middot; {FERN["--font-sans"]}</div></div>'
    '<div class="ds-cell"><div style="font-family: var(--font-mono); font-size: var(--fs-lead);">'
    "ui-monospace — 1234567890</div>"
    f'<div class="ds-caption">--font-mono &middot; {FERN["--font-mono"]}</div></div>'
    "</div>\n"
    '<div class="ds-group-title">Size scale (--fs-*)</div>\n'
    + "\n".join(typ_rows)
    + '\n<div class="ds-group-title">Weights (--fw-*)</div>\n'
    + "\n".join(fw_rows)
)
(OUT / "typography.html").write_text(page(
    '@dsCard group="Foundations" name="Type scale"',
    "Type scale — keepbook design tokens",
    "", typo_body))

# ---------------------------------------------------------------- buttons.html
def buttons_inner():
    return """<div class="ds-group-title">control-button</div>
<div class="ds-row">
  <button class="control-button">Default</button>
  <button class="control-button selected">Selected</button>
  <button class="control-button" disabled>Disabled</button>
  <button class="control-button focus-chip">Focus chip</button>
  <button class="control-button danger">Danger</button>
  <button class="control-button danger-button">Danger button</button>
</div>
<div class="ds-group-title">icon-button</div>
<div class="ds-row">
  <button class="icon-button" title="Edit">&#9998;</button>
  <button class="icon-button" disabled title="Disabled">&#9998;</button>
  <button class="icon-button add-location-button" title="Add location">+</button>
</div>
<div class="ds-caption" style="margin:-6px 0 14px;">icon-button &middot; icon-button (disabled) &middot; add-location-button</div>
<div class="ds-group-title">file-select-button</div>
<div class="ds-row">
  <label class="file-select-wrapper">
    <input class="file-select-input" type="file" tabindex="-1" aria-hidden="true">
    <span class="file-select-button">Select key file</span>
  </label>
</div>
<div class="ds-group-title">nav-button</div>
<div class="ds-col" style="max-width:220px;">
  <button class="nav-button">Accounts</button>
  <button class="nav-button selected">Net worth</button>
  <button class="nav-button">Spending</button>
</div>"""

buttons_body = (
    '<div class="ds-h1">Buttons</div>\n'
    '<p class="ds-sub">Interactive control classes from <code>views/shared.rs</code> and the settings views, '
    "in every documented state. Hover states render on pointer devices.</p>\n"
    + two_themes(buttons_inner)
)
(OUT / "buttons.html").write_text(page(
    '@dsCard group="Components" name="Buttons"',
    "Buttons — keepbook design tokens",
    "", buttons_body))

# ---------------------------------------------------------------- panels.html
def panels_inner():
    return """<section class="panel">
  <div class="panel-header">
    <div class="panel-title">
      <h2>Net worth</h2>
      <span>Across 6 accounts</span>
    </div>
    <div class="settings-actions inline-actions">
      <button class="control-button selected">1Y</button>
      <button class="control-button">All</button>
    </div>
  </div>
  <div class="summary-grid" style="grid-template-columns: repeat(auto-fit, minmax(150px,1fr));">
    <article class="metric">
      <span class="metric-label">Current</span>
      <strong>$128,400</strong>
      <small>as of Jul 12</small>
    </article>
    <article class="metric">
      <span class="metric-label">Range change</span>
      <strong class="change-positive">+$12,240</strong>
      <small>trailing 12 months</small>
    </article>
  </div>
</section>
<div class="backend-activity" role="status">
  <span class="activity-spinner"></span>
  <span>Syncing Chase connection</span>
</div>
<section class="status-panel" style="margin-bottom:16px;">
  <h2>Connection</h2>
  <p>Loading local finance data...</p>
</section>
<section class="panel">
  <div class="inline-status">
    <h2>No data yet</h2>
    <p>Add a connection to start tracking balances.</p>
  </div>
</section>
<section class="panel settings-panel">
  <div class="panel-header">
    <div class="panel-title"><h2>Portfolio</h2></div>
    <span>keepbook.toml</span>
  </div>
  <div class="settings-list">
    <article class="setting-row">
      <div class="setting-copy">
        <strong>Latent capital gains tax</strong>
        <small>Include brokerage in net worth and history</small>
      </div>
      <label class="switch-control">
        <input type="checkbox" checked>
        <span class="switch-track"><span class="switch-thumb"></span></span>
      </label>
    </article>
    <article class="setting-row">
      <div class="setting-copy">
        <strong>Show borderline recurring</strong>
        <small>Include low-confidence matches</small>
      </div>
      <label class="switch-control">
        <input type="checkbox">
        <span class="switch-track"><span class="switch-thumb"></span></span>
      </label>
    </article>
  </div>
</section>"""

panels_body = (
    '<div class="ds-h1">Panels &amp; cards</div>\n'
    '<p class="ds-sub">Panel/header/title composition, metric cards, status &amp; inline-status blocks, the '
    "backend-activity notice, and settings setting-row switches (checked + unchecked). Markup mirrors "
    "<code>views/shared.rs</code> and <code>views/graph_settings.rs</code>.</p>\n"
    + two_themes(panels_inner)
)
(OUT / "panels.html").write_text(page(
    '@dsCard group="Components" name="Panels & cards"',
    "Panels & cards — keepbook design tokens",
    "", panels_body))

# ---------------------------------------------------------------- badges.html
def badges_inner():
    return """<div class="ds-group-title">status</div>
<div class="ds-row">
  <span class="status">Active</span>
  <span class="status liability-status">Liability</span>
  <span class="status ignored-status">Ignored</span>
</div>
<div class="ds-group-title">review-badge</div>
<div class="ds-row">
  <span class="review-badge review-proposed">Proposed</span>
  <span class="review-badge review-verified">Verified</span>
  <span class="review-badge review-dismissed">Dismissed</span>
</div>
<div class="ds-group-title">reason-chip</div>
<div class="ds-row">
  <span class="reason-chip">monthly cadence</span>
  <span class="reason-chip">stable amount</span>
</div>
<div class="ds-group-title">tag-pill / tag-suggestion-pill</div>
<div class="ds-row">
  <button class="tag-pill removable"><span>groceries</span><span class="tag-pill-remove">x</span></button>
  <button class="tag-pill removable"><span>utilities</span><span class="tag-pill-remove">x</span></button>
  <button class="tag-suggestion-pill"><span>dining</span></button>
  <button class="tag-suggestion-pill"><span>travel</span></button>
</div>
<div class="ds-group-title">ignored-badge</div>
<div class="ds-row">
  <span class="ignored-badge">Not counted</span>
</div>
<div class="ds-group-title">change-positive / change-negative</div>
<div class="ds-row">
  <strong class="change-positive">+$1,240.00</strong>
  <strong class="change-negative">-$320.50</strong>
</div>"""

badges_body = (
    '<div class="ds-h1">Badges &amp; pills</div>\n'
    '<p class="ds-sub">Status badges, recurring review badges, reason chips, tag pills, the ignored badge, and '
    "signed change values. Markup mirrors the accounts, recurring, and spending views.</p>\n"
    + two_themes(badges_inner)
)
(OUT / "badges.html").write_text(page(
    '@dsCard group="Components" name="Badges & pills"',
    "Badges & pills — keepbook design tokens",
    "", badges_body))

# ---------------------------------------------------------------- forms.html
def forms_inner():
    return """<div class="ds-group-title">control-field / control-input</div>
<div class="control-grid" style="grid-template-columns: repeat(auto-fit, minmax(150px,1fr));">
  <label class="control-field">
    <span>Label</span>
    <input class="control-input" type="text" value="Chase Reserve" placeholder="Text">
  </label>
  <label class="control-field">
    <span>Start date</span>
    <input class="control-input" type="date" value="2025-01-01">
  </label>
  <label class="control-field">
    <span>Amount</span>
    <input class="control-input" type="number" value="1200" step="0.01">
  </label>
</div>
<div class="ds-group-title">transaction-search-input</div>
<div class="ds-row">
  <input class="transaction-search-input" type="text" placeholder="Search transactions" value="">
</div>
<div class="ds-group-title">ai-rule-prompt</div>
<div class="ai-rule-panel">
  <textarea class="ai-rule-prompt" placeholder="Describe a tagging rule in plain language">Tag anything from Whole Foods as groceries</textarea>
  <div class="ai-rule-status">Draft &middot; not yet applied</div>
</div>
<div class="ds-group-title">toggle-field / compact-check</div>
<div class="ds-row">
  <label class="toggle-field"><input type="checkbox" checked><span>Borderline</span></label>
  <label class="toggle-field"><input type="checkbox"><span>Dismissed</span></label>
  <label class="compact-check"><input type="checkbox" checked><span>Stale only</span></label>
  <label class="compact-check"><input type="checkbox"><span>Full transactions</span></label>
</div>
<div class="ds-group-title">directory-picker</div>
<div class="control-field" style="margin-bottom:14px;">
  <span>Data directory</span>
  <div class="directory-picker">
    <code class="directory-picker-path">/home/imalison/.local/share/keepbook-data</code>
    <button class="control-button">Use app data folder</button>
  </div>
</div>
<div class="ds-group-title">indeterminate-progress</div>
<div class="clone-progress-copy" style="margin-bottom:16px;">
  <div class="indeterminate-progress"><span></span></div>
</div>
<div class="ds-group-title">activity-spinner</div>
<div class="ds-row">
  <span class="activity-spinner"></span>
  <span class="activity-spinner large"></span>
</div>"""

forms_body = (
    '<div class="ds-h1">Forms &amp; inputs</div>\n'
    '<p class="ds-sub">Input controls from the shared inputs, spending, and settings views: labelled fields, '
    "search box, AI-rule textarea, toggles, directory picker, and progress indicators.</p>\n"
    + two_themes(forms_inner)
)
(OUT / "forms.html").write_text(page(
    '@dsCard group="Components" name="Forms & inputs"',
    "Forms & inputs — keepbook design tokens",
    "", forms_body))

# ---------------------------------------------------------------- charts.html
SERIES = [f"--series-{i}" for i in range(1, 19)]
CATS = [f"--cat-{i}" for i in range(1, 11)]

def palette_swatches(tokens):
    cells = []
    for t in tokens:
        v = FERN[t]
        cells.append(
            '<div class="ds-swatch">'
            f'<div class="ds-chip" style="background: var({t});"></div>'
            f'<div class="ds-name">{t}</div><div class="ds-val">{v}</div></div>'
        )
    return '<div class="ds-swatches">' + "".join(cells) + "</div>"

CHART_SVG = """<svg viewBox="0 0 340 180" role="img" aria-label="Sample net-worth line chart" style="width:100%;height:auto;display:block;">
  <line class="chart-grid" x1="12" y1="20" x2="328" y2="20" />
  <line class="chart-grid" x1="12" y1="60" x2="328" y2="60" />
  <line class="chart-grid" x1="12" y1="100" x2="328" y2="100" />
  <line class="chart-grid axis" x1="12" y1="150" x2="328" y2="150" />
  <path class="chart-area" d="M12,120 L64,92 L118,102 L172,64 L224,74 L276,44 L328,32 L328,150 L12,150 Z" />
  <path class="chart-line" d="M12,120 L64,92 L118,102 L172,64 L224,74 L276,44 L328,32" />
  <circle class="chart-point" cx="328" cy="32" r="4" />
  <text class="chart-axis-label" x="12" y="168">Jan</text>
  <text class="chart-axis-label end" x="328" y="168">Dec</text>
  <text class="chart-axis-label" x="12" y="16">$130k</text>
</svg>"""

LEGEND = """<div class="stacked-legend">
  <button class="stacked-legend-item selected">
    <span class="stacked-legend-swatch" style="background: var(--series-1);"></span><span>Brokerage</span>
  </button>
  <button class="stacked-legend-item">
    <span class="stacked-legend-swatch" style="background: var(--series-2);"></span><span>Checking</span>
  </button>
  <button class="stacked-legend-item">
    <span class="stacked-legend-swatch" style="background: var(--series-3);"></span><span>Savings</span>
  </button>
  <button class="stacked-legend-item asset">
    <span class="stacked-legend-swatch" style="background: var(--series-4);"></span><span>Mortgage</span>
  </button>
</div>"""

TOOLTIP_SVG = """<svg viewBox="0 0 200 96" role="img" aria-label="Chart tooltip mock" style="width:200px;max-width:100%;height:auto;display:block;">
  <rect class="chart-tooltip" x="1" y="1" rx="6" ry="6" width="150" height="82" />
  <text class="chart-tooltip-date" x="14" y="24">Mar 14, 2025</text>
  <text class="chart-tooltip-value" x="14" y="46">$128,400</text>
  <text class="chart-tooltip-detail" x="14" y="66">Brokerage &middot; +$1,240</text>
</svg>"""

def charts_inner():
    return (
        '<div class="ds-group-title">Line chart (chart-grid / chart-area / chart-line / chart-axis-label)</div>\n'
        + CHART_SVG
        + '\n<div class="ds-group-title">stacked-legend</div>\n'
        + LEGEND
        + '\n<div class="ds-group-title">chart-tooltip mock</div>\n'
        + TOOLTIP_SVG
    )

charts_body = (
    '<div class="ds-h1">Chart tokens &amp; palettes</div>\n'
    '<p class="ds-sub">Categorical palettes plus live chart chrome. Palette values are theme-independent '
    "(defined only in the default block); the line chart, legend, and tooltip are rendered under both themes so the "
    "grid, area, line, and tooltip roles read in each. SVG fills come from CSS classes or <code>style</code> "
    "custom-property refs, never presentation attributes.</p>\n"
    '<div class="ds-group-title">Stacked / net-worth series (--series-1 &hellip; --series-18)</div>\n'
    + palette_swatches(SERIES)
    + '\n<div class="ds-group-title">Spending-tag palette (--cat-1 &hellip; --cat-10)</div>\n'
    + palette_swatches(CATS)
    + "\n" + two_themes(charts_inner)
)
(OUT / "charts.html").write_text(page(
    '@dsCard group="Charts" name="Chart tokens & palettes"',
    "Chart tokens & palettes — keepbook design tokens",
    "", charts_body))

# Add scoped theme CSS to every page that uses two_themes by appending to their extra-style.
# Simpler: regenerate those pages with the scoped block already in SCAFFOLD? We keep it per-page:
for fn in ["colors.html","buttons.html","panels.html","badges.html","forms.html","charts.html"]:
    p = OUT / fn
    txt = p.read_text()
    # inject scoped theme css right before the closing of the second <style>
    inject = "\n" + scoped_theme_css()
    # second </style> is the scaffold one; insert before the first occurrence after SCAFFOLD marker.
    idx = txt.index("</style>\n</head>")
    txt = txt[:idx] + inject + txt[idx:]
    p.write_text(txt)

print("wrote:", sorted(x.name for x in OUT.glob("*.html")))
