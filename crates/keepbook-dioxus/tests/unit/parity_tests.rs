//! Markup parity between the Dioxus components and their React mirrors.
//!
//! Each case renders a component to HTML and compares it with
//! `design/parity/<Component>.<case>.html`. Run with `UPDATE_PARITY=1` to
//! rewrite those files after an intentional markup change; the design
//! package's `npm test` then checks the React mirrors against them.

use crate::components::*;
use crate::dto::*;
use crate::logic::{HistoryChangeSummary, ThemeSettings};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

fn tag_colors() -> HashMap<String, &'static str> {
    HashMap::new()
}

fn spending_entry(key: &str, total: &str, count: usize) -> SpendingBreakdownEntry {
    SpendingBreakdownEntry {
        key: key.to_string(),
        total: total.to_string(),
        transaction_count: count,
    }
}

type Case = (&'static str, fn() -> Element);

const CASES: &[Case] = &[
    ("AppShell.basic", || {
        rsx! {
            AppShell {
                title: "Keepbook",
                currency: "USD",
                repositories: vec![
                    SelectOption::new("personal", "Personal"),
                    SelectOption { value: "parents".into(), label: "Parents".into(), disabled: true },
                ],
                repository: "personal",
                nav_items: vec![
                NavItem { label: "Accounts".into(), icon: ButtonIcon::Wallet },
                NavItem { label: "Spending".into(), icon: ButtonIcon::Receipt },
            ],
                active: "Accounts",
                p { "Content" }
            }
        }
    }),
    ("Modal.basic", || {
        rsx! {
            Modal {
                title: "Clone repository",
                header_actions: rsx! { IconButton { label: "Close", glyph: "×", onclick: |_| {} } },
                actions: rsx! { ControlButton { danger: true, onclick: |_| {}, "Cancel" } },
                Progress { label: "Cloning…" }
            }
        }
    }),
    ("Modal.wide", || {
        rsx! {
            Modal { title: "Group edit", wide: true, p { "Body" } }
        }
    }),
    ("PageToolbar.basic", || {
        rsx! {
            PageToolbar {
                ControlButton { icon: ButtonIcon::GitBranch, onclick: |_| {}, "Git sync" }
                ControlButton { primary: true, icon: ButtonIcon::Refresh, onclick: |_| {}, "Refresh prices" }
            }
        }
    }),
    ("Panel.basic", || {
        rsx! {
            Panel {
                title: "Accounts",
                subtitle: "2",
                actions: rsx! { ControlButton { onclick: |_| {}, "Refresh" } },
                p { "Body" }
            }
        }
    }),
    ("Panel.title_only", || {
        rsx! {
            Panel { title: "Accounts", class: "assets-panel", p { "Body" } }
        }
    }),
    ("SummaryGrid.basic", || {
        rsx! {
            SummaryGrid {
                MetricCard { label: "Net worth", value: "$6,814.24", detail: "2026-10-08" }
                MetricCard { label: "Accounts", value: "3", detail: "3 total" }
            }
        }
    }),
    ("ThemePicker.basic", || rsx! { ThemePicker {} }),
    ("ThemeOptions.fern_system", || {
        rsx! {
            ThemeOptions { settings: ThemeSettings::default(), wallpaper: false, onchange: |_| {} }
        }
    }),
    ("ThemeOptions.dynamic_seed", || {
        rsx! {
            ThemeOptions {
                settings: ThemeSettings { palette: "dynamic".into(), mode: "dark".into(), seed: Some("#6750a4".into()) },
                wallpaper: false,
                onchange: |_| {},
            }
        }
    }),
    ("ThemeOptions.dynamic_wallpaper", || {
        rsx! {
            ThemeOptions {
                settings: ThemeSettings { palette: "dynamic".into(), mode: "system".into(), seed: None },
                wallpaper: true,
                onchange: |_| {},
            }
        }
    }),
    (
        "IconSvg.refresh",
        || rsx! { IconSvg { icon: ButtonIcon::Refresh } },
    ),
    ("ControlButton.primary_icon", || {
        rsx! {
            ControlButton { primary: true, icon: ButtonIcon::Refresh, title: "Fetch prices", onclick: |_| {}, "Refresh prices" }
        }
    }),
    ("ControlButton.busy", || {
        rsx! {
            ControlButton { busy: true, icon: ButtonIcon::Refresh, onclick: |_| {}, "Refreshing" }
        }
    }),
    ("ControlButton.danger_small", || {
        rsx! {
            ControlButton { danger: true, small: true, disabled: true, onclick: |_| {}, "Dismiss" }
        }
    }),
    ("ControlButton.selected", || {
        rsx! {
            ControlButton { selected: true, onclick: |_| {}, "Monthly" }
        }
    }),
    ("SplitButton.closed", || {
        rsx! {
            SplitButton {
                primary: true,
                icon: ButtonIcon::Refresh,
                menu_label: "More refresh options",
                actions: vec![MenuAction { value: "stale", label: "Refresh stale prices", detail: "Skip fresh prices" }],
                onclick: |_| {},
                onselect: |_| {},
                "Refresh all prices"
            }
        }
    }),
    ("IconButton.close", || {
        rsx! {
            IconButton { label: "Close", glyph: "×", onclick: |_| {} }
        }
    }),
    ("IconButton.toggle", || {
        rsx! {
            IconButton { label: "Expand to edit", glyph: "›", class: "transaction-expand-toggle", disabled: true, onclick: |_| {} }
        }
    }),
    ("SegmentedControl.range", || {
        rsx! {
            SegmentedControl {
                label: "Range",
                options: vec![SegmentedOption::new("30d", "30D"), SegmentedOption::new("1y", "1Y")],
                selected: "1y",
                onselect: |_| {},
            }
        }
    }),
    ("FilterChip.basic", || {
        rsx! {
            FilterChip { label: "Focused: Groceries", title: "Clear the focused tag", onclear: |_| {} }
        }
    }),
    ("TextInput.labeled_number", || {
        rsx! {
            TextInput { label: "Min", kind: InputKind::Number, value: "0.70", oninput: |_| {} }
        }
    }),
    ("TextInput.search", || {
        rsx! {
            TextInput { kind: InputKind::Search, class: "transaction-search-input", value: "", placeholder: "Filter titles", oninput: |_| {} }
        }
    }),
    ("TextInput.date_small", || {
        rsx! {
            TextInput { label: "Start", kind: InputKind::Date, small: true, value: "2026-07-10", min: "2026-01-01", max: "2026-12-31", oninput: |_| {} }
        }
    }),
    ("TextInput.multiline", || {
        rsx! {
            TextInput { multiline: true, value: "", placeholder: "Ask for a rule", disabled: true, oninput: |_| {} }
        }
    }),
    ("Select.labeled", || {
        rsx! {
            Select { label: "Sort", options: vec![SelectOption::new("cost", "Annual cost"), SelectOption::new("name", "Name")], value: "name", onchange: |_| {} }
        }
    }),
    ("Select.bare", || {
        rsx! {
            Select { options: vec![SelectOption::new("auto", "Auto")], value: "auto", disabled: true, onchange: |_| {} }
        }
    }),
    ("Checkbox.checked", || {
        rsx! {
            Checkbox { label: "Include", class: "account-include-toggle", checked: true, onchange: |_| {} }
        }
    }),
    ("Checkbox.disabled", || {
        rsx! {
            Checkbox { label: "Stale only", checked: false, disabled: true, onchange: |_| {} }
        }
    }),
    ("Switch.on", || {
        rsx! {
            Switch { label: "Start minimized to tray", checked: true, onchange: |_| {} }
        }
    }),
    ("SettingRow.stacked", || {
        rsx! {
            SettingRow { title: "Window decorations", description: "Auto hides the title bar", stacked: true, p { "Control" } }
        }
    }),
    ("Badge.neutral", || rsx! { Badge { "Not counted" } }),
    (
        "Badge.positive",
        || rsx! { Badge { tone: BadgeTone::Positive, "Active" } },
    ),
    (
        "Badge.negative",
        || rsx! { Badge { tone: BadgeTone::Negative, "Dismissed" } },
    ),
    (
        "Badge.warning",
        || rsx! { Badge { tone: BadgeTone::Warning, "Liability" } },
    ),
    ("TagPill.readonly", || rsx! { TagPill { tag: "Groceries" } }),
    (
        "TagPill.removable",
        || rsx! { TagPill { tag: "Dining", removable: true, onclick: |_| {} } },
    ),
    (
        "TagPill.suggestion",
        || rsx! { TagPill { tag: "Travel", suggestion: true, disabled: true, onclick: |_| {} } },
    ),
    ("MetricCard.basic", || {
        rsx! {
            MetricCard { label: "Net worth", value: "$6,814.24", detail: "2026-10-08" }
        }
    }),
    ("DataTable.basic", || {
        rsx! {
            DataTable {
                class: "account-table",
                columns: vec!["Account".to_string(), "Balance".to_string()],
                div { class: "table-row",
                    span { strong { "Checking" } }
                    span { "$8,250.44" }
                }
            }
        }
    }),
    ("TreeGroup.basic", || {
        rsx! {
            TreeGroup {
                title: "Demo Bank",
                subtitle: "manual",
                aside: rsx! { Badge { tone: BadgeTone::Positive, "2/2 active" } },
                p { "Rows" }
            }
        }
    }),
    (
        "OperationStatus.busy",
        || rsx! { OperationStatus { message: "Refreshing prices…", busy: true } },
    ),
    (
        "OperationStatus.done",
        || rsx! { OperationStatus { message: "Refreshed 14 prices.", busy: false } },
    ),
    (
        "InlineStatus.basic",
        || rsx! { InlineStatus { title: "Couldn't load", message: "Try again." } },
    ),
    (
        "EmptyState.basic",
        || rsx! { EmptyState { title: "No assets", detail: "Refresh balances." } },
    ),
    (
        "EmptyState.loading",
        || rsx! { EmptyState { title: "Updating graph", detail: "1Y / Weekly", loading: true } },
    ),
    ("EmptyState.compact", || {
        rsx! {
            EmptyState { title: "No pending edits", detail: "Nothing queued.", compact: true, class: "spending-over-time-empty" }
        }
    }),
    (
        "Spinner.small",
        || rsx! { Spinner { size: SpinnerSize::Small } },
    ),
    ("Spinner.medium", || rsx! { Spinner {} }),
    (
        "Spinner.large",
        || rsx! { Spinner { size: SpinnerSize::Large } },
    ),
    (
        "Progress.busy",
        || rsx! { Progress { label: "Cloning keepbook-data…" } },
    ),
    (
        "Progress.done",
        || rsx! { Progress { label: "Clone complete.", busy: false } },
    ),
    ("Legend.static", || {
        rsx! {
            Legend {
                items: vec![
                    LegendItem { label: "Checking".into(), color: "var(--series-1)".into(), muted: false },
                    LegendItem { label: "Brokerage".into(), color: "var(--series-2)".into(), muted: true },
                ],
            }
        }
    }),
    ("Legend.selectable", || {
        rsx! {
            Legend {
                class: "spending-bar-legend",
                items: vec![
                    LegendItem { label: "Housing".into(), color: "var(--series-1)".into(), muted: false },
                    LegendItem { label: "Dining".into(), color: "var(--series-2)".into(), muted: false },
                ],
                selected: "Dining",
                onselect: |_| {},
            }
        }
    }),
    ("NetWorthChart.basic", || {
        rsx! {
            NetWorthChart {
                data: vec![
                    NetWorthDataPoint { date: "2026-01-01".into(), value: 5000.0 },
                    NetWorthDataPoint { date: "2026-02-01".into(), value: 5600.5 },
                    NetWorthDataPoint { date: "2026-03-01".into(), value: 5400.0 },
                ],
                currency: "USD",
                empty_title: "No net worth history",
                empty_detail: "Refresh balances.",
                current_value_text: "5400",
                change_summary: HistoryChangeSummary { class: "change-positive", text: "+$400.00 (8.0%)".into() },
                onselectrange: |_| {},
            }
        }
    }),
    ("NetWorthChart.empty", || {
        rsx! {
            NetWorthChart {
                data: vec![],
                currency: "USD",
                empty_title: "No net worth history",
                empty_detail: "Refresh balances.",
                current_value_text: "0",
                change_summary: HistoryChangeSummary { class: "", text: String::new() },
                onselectrange: |_| {},
            }
        }
    }),
    ("SpendingChart.basic", || {
        let spending: SpendingOutput = serde_json::from_value(serde_json::json!({
            "currency": "USD", "tz": "UTC", "start_date": "2026-04-01", "end_date": "2026-05-31",
            "period": "monthly", "total": "-950", "transaction_count": 3,
            "skipped_transaction_count": 0, "missing_price_transaction_count": 0, "missing_fx_transaction_count": 0,
            "periods": [
                { "start_date": "2026-04-01", "end_date": "2026-04-30", "total": "-700", "transaction_count": 2,
                  "breakdown": [{ "key": "Housing", "total": "-600", "transaction_count": 1 }, { "key": "Dining", "total": "-100", "transaction_count": 1 }] },
                { "start_date": "2026-05-01", "end_date": "2026-05-31", "total": "-250", "transaction_count": 1,
                  "breakdown": [{ "key": "Dining", "total": "-250", "transaction_count": 1 }] }
            ]
        }))
        .expect("spending fixture should parse");
        rsx! {
            SpendingChart {
                spending,
                series: vec![spending_entry("Housing", "-600", 1), spending_entry("Dining", "-350", 2)],
                bucket_label: "Monthly",
                colors: tag_colors(),
                onclick: |_| {},
                onfocussegment: |_| {},
                onselecttag: |_| {},
            }
        }
    }),
    ("SpendingBreakdown.basic", || {
        rsx! {
            SpendingBreakdown {
                tags: vec![spending_entry("Housing", "-600", 1), spending_entry("Dining", "-350", 2)],
                colors: tag_colors(),
                currency: "USD",
                totals: vec![SpendingTotal { label: "Total".into(), value: "$950.00".into(), detail: "3 transactions".into(), highlighted: false }],
                selected: "Dining",
                onselect: |_| {},
            }
        }
    }),
];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("design/parity")
}

#[test]
fn components_match_their_parity_snapshots() {
    let dir = golden_dir();
    let update = std::env::var_os("UPDATE_PARITY").is_some();
    if update {
        std::fs::create_dir_all(&dir).expect("parity dir should be creatable");
        for entry in std::fs::read_dir(&dir).expect("parity dir should be readable") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_some_and(|ext| ext == "html") {
                std::fs::remove_file(path).expect("stale snapshot should be removable");
            }
        }
    }
    let mut stale = Vec::new();
    for (name, case) in CASES {
        let html = render(*case);
        let path = dir.join(format!("{name}.html"));
        if update {
            std::fs::write(&path, format!("{html}\n")).expect("snapshot should be writable");
        } else if std::fs::read_to_string(&path).ok().as_deref()
            != Some(format!("{html}\n").as_str())
        {
            stale.push(*name);
        }
    }
    assert!(
        stale.is_empty(),
        "component markup changed for {stale:?}; run `UPDATE_PARITY=1 cargo test -p keepbook-dioxus parity`, \
         then `npm test` in crates/keepbook-dioxus/design to update the React mirrors"
    );
    let snapshots = std::fs::read_dir(&dir)
        .expect("parity dir should exist")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "html").then_some(())?;
            path.file_stem()?.to_str().map(str::to_string)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let cases = CASES.iter().map(|(name, _)| name.to_string()).collect();
    assert_eq!(
        snapshots, cases,
        "every snapshot file must belong to a parity case"
    );
}

#[test]
fn every_component_has_a_parity_case() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/components");
    let mut missing = Vec::new();
    for group in std::fs::read_dir(&dir).expect("components dir") {
        let group = group.expect("entry").path();
        if !group.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&group).expect("group dir") {
            let source =
                std::fs::read_to_string(file.expect("entry").path()).expect("component source");
            for line in source.lines() {
                if let Some(rest) = line.strip_prefix("pub(crate) fn ") {
                    let name = rest.split('(').next().unwrap_or_default();
                    let is_component = name.chars().next().is_some_and(char::is_uppercase);
                    let prefix = format!("{name}.");
                    if is_component && !CASES.iter().any(|(case, _)| case.starts_with(&prefix)) {
                        missing.push(name.to_string());
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "components without a parity case: {missing:?}"
    );
}

/// Every `ButtonIcon`; the match stops compiling when a variant is added.
fn every_button_icon() -> [ButtonIcon; 12] {
    use ButtonIcon::*;
    match Refresh {
        Refresh | GitBranch | ChevronDown | Wallet | Landmark | Receipt | Repeat | TrendingUp
        | Layers | Plug | FilePen | Settings => {}
    }
    [
        Refresh,
        GitBranch,
        ChevronDown,
        Wallet,
        Landmark,
        Receipt,
        Repeat,
        TrendingUp,
        Layers,
        Plug,
        FilePen,
        Settings,
    ]
}

#[test]
fn every_icon_is_defined() {
    let icons: HashMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../../assets/icons.json")).expect("icons.json");
    let mut listed = icons.keys().map(String::as_str).collect::<Vec<_>>();
    let mut variants = every_button_icon().map(ButtonIcon::name).to_vec();
    listed.sort();
    variants.sort();
    assert_eq!(
        listed, variants,
        "assets/icons.json must define exactly the ButtonIcon variants"
    );
    assert!(
        icons.values().all(|paths| !paths.is_empty()),
        "every icon needs paths"
    );
}

#[test]
fn react_nav_items_match_the_app_views() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("design/src/layout/AppShell.tsx"),
    )
    .expect("AppShell mirror should be readable");
    let list = source
        .split_once("NAV_ITEMS: NavItem[] = [")
        .and_then(|(_, rest)| rest.split_once("];"))
        .map(|(list, _)| list)
        .expect("AppShell.tsx should define NAV_ITEMS");
    let quoted: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
    let react: Vec<(&str, &str)> = quoted.chunks(2).map(|pair| (pair[0], pair[1])).collect();
    let app: Vec<(&str, &str)> = crate::views::ActiveView::ALL
        .map(|view| (view.label(), view.icon().name()))
        .to_vec();
    assert_eq!(
        react, app,
        "NAV_ITEMS in AppShell.tsx must list the app's views and icons in order"
    );
}
