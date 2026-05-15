use super::*;
use crate::app::TransactionAnnotationOutput;
use crate::config::{
    DisplayConfig, GitConfig, IgnoreConfig, RefreshConfig, SpendingConfig, TrayConfig,
};
use serde_json::json;
use std::path::PathBuf;

fn tx(id: &str, timestamp: &str, amount: &str) -> TransactionOutput {
    TransactionOutput {
        id: id.to_string(),
        account_id: "acct-1".to_string(),
        account_name: "Checking".to_string(),
        timestamp: timestamp.to_string(),
        description: "desc".to_string(),
        amount: amount.to_string(),
        asset: json!({"type":"currency","iso_code":"USD"}),
        status: "posted".to_string(),
        tags: Vec::new(),
        subtags: Vec::new(),
        annotation: None,
        standardized_metadata: None,
    }
}

fn test_config() -> ResolvedConfig {
    ResolvedConfig {
        data_dir: PathBuf::from("/tmp"),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: crate::config::HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: GitConfig::default(),
    }
}

#[test]
fn compare_by_amount_handles_numeric_values() {
    let a = tx("a", "2026-01-01T00:00:00+00:00", "12");
    let b = tx("b", "2026-01-01T00:00:00+00:00", "-5");
    assert_eq!(
        compare_transactions(&a, &b, SortMode::AmountAsc),
        Ordering::Greater
    );
    assert_eq!(
        compare_transactions(&a, &b, SortMode::AmountDesc),
        Ordering::Less
    );
}

#[test]
fn timespan_filter_respects_cutoff() {
    let mut state = AppState::new(
        vec![
            tx("a", "2026-01-01T00:00:00+00:00", "1"),
            tx("b", "2026-02-01T00:00:00+00:00", "1"),
        ],
        TransactionTagMatcher::default(),
        PathBuf::from("/tmp/transaction-rules-test.jsonl"),
        false,
        TuiOptions::default(),
    );
    state.span = TimeSpan::All;
    state.recompute_visible_transactions();
    let all_len = state.visible_transaction_indices.len();
    state.span = TimeSpan::Days7;
    state.recompute_visible_transactions();
    assert!(state.visible_transaction_indices.len() <= all_len);
}

#[test]
fn display_uses_annotation_description_when_present() {
    let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1");
    t.annotation = Some(TransactionAnnotationOutput {
        description: Some("override".to_string()),
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
    });
    let description = t
        .annotation
        .as_ref()
        .and_then(|ann| ann.description.as_deref())
        .unwrap_or(t.description.as_str());
    assert_eq!(description, "override");
}

#[test]
fn display_uses_annotation_tag_when_present() {
    let matcher = TransactionTagMatcher::default();
    let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1");
    assert_eq!(transaction_tag_string(&t, &matcher), "-");

    t.tags = vec!["Groceries".to_string()];
    assert_eq!(transaction_tag_string(&t, &matcher), "Groceries");

    t.annotation = Some(TransactionAnnotationOutput {
        description: None,
        note: None,
        tags: Some(vec!["food".to_string()]),
        subtags: None,
        effective_date: None,
    });
    assert_eq!(transaction_tag_string(&t, &matcher), "food");
}

#[test]
fn display_uses_rule_tag_when_annotation_missing() {
    let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1");
    t.description = "Starbucks #123".to_string();

    let rule = TransactionTagRule {
        set_description: None,
        set_tags: Some(vec!["coffee".to_string()]),
        set_subtags: None,
        match_account_id: None,
        match_account_name: exact_ci_regex_pattern("Checking"),
        match_description: Some("(?i)^starbucks".to_string()),
        match_tag: None,
        match_subtag: None,
        match_status: None,
        match_amount: None,
    };
    let matcher = TransactionTagMatcher {
        rules: vec![CompiledTransactionTagRule::from_rule(0, &rule).expect("valid rule")],
    };

    assert_eq!(transaction_tag_string(&t, &matcher), "coffee");
}

#[test]
fn fallback_regex_suggestion_normalizes_whitespace() {
    assert_eq!(
        fallback_regex_suggestion("  coffee   shop  purchase "),
        "(?i)^coffee\\s+shop\\s+purchase$"
    );
}

#[test]
fn filtered_tag_suggestions_prefers_prefix_matches() {
    let catalog = vec![
        "Groceries".to_string(),
        "Coffee".to_string(),
        "Dining Out".to_string(),
        "Office Coffee".to_string(),
    ];
    let out = filtered_tag_suggestions(&catalog, "cof");
    assert_eq!(
        out,
        vec!["Coffee".to_string(), "Office Coffee".to_string(),]
    );
}

#[test]
fn selected_tag_from_modal_uses_active_selection() {
    let modal = TagModalState {
        action: TagAction::OneOff {
            source: SelectedTransactionInfo {
                account_id: "acct-1".to_string(),
                account_name: "Checking".to_string(),
                transaction_id: "tx-1".to_string(),
                status: "posted".to_string(),
                amount: "-1".to_string(),
                description: "Coffee".to_string(),
            },
        },
        input: "din".to_string(),
        cursor: 3,
        suggestions: vec!["Dining".to_string()],
        selected_suggestion: 0,
        selection_active: true,
    };
    assert_eq!(selected_tag_from_modal(&modal), "Dining".to_string());
}

#[test]
fn apply_text_input_edit_supports_cursor_navigation_and_insert_delete() {
    let mut input = "abc".to_string();
    let mut cursor = 3usize;

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Left
    ));
    assert_eq!(cursor, 2);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Char('X')
    ));
    assert_eq!(input, "abXc");
    assert_eq!(cursor, 3);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Delete
    ));
    assert_eq!(input, "abX");
    assert_eq!(cursor, 3);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Backspace
    ));
    assert_eq!(input, "ab");
    assert_eq!(cursor, 2);
}

#[test]
fn asset_label_normalizes_currency_codes() {
    assert_eq!(
        asset_label(&json!({"type":"currency","iso_code":"840"})),
        "USD"
    );
    assert_eq!(
        asset_label(&json!({"type":"currency","iso_code":"usd"})),
        "USD"
    );
}

#[test]
fn amount_display_uses_formatter_for_reporting_currency() {
    let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1234.5");
    t.asset = json!({"type":"currency","iso_code":"usd"});
    let mut config = test_config();
    config.display.currency_decimals = Some(2);
    config.display.currency_grouping = true;
    config.display.currency_symbol = Some("$".to_string());
    config.display.currency_fixed_decimals = true;

    assert_eq!(transaction_amount_string(&t, &config), "$1,234.50");
}

#[test]
fn amount_display_normalizes_non_reporting_assets_without_symbol() {
    let mut t = tx("a", "2026-02-01T00:00:00+00:00", "1.2300");
    t.asset = json!({"type":"crypto","symbol":"BTC"});
    let mut config = test_config();
    config.display.currency_decimals = Some(2);
    config.display.currency_grouping = true;
    config.display.currency_symbol = Some("$".to_string());
    config.display.currency_fixed_decimals = true;

    assert_eq!(transaction_amount_string(&t, &config), "1.23");
}

#[test]
fn amount_display_preserves_unparseable_values() {
    let t = tx("a", "2026-02-01T00:00:00+00:00", "not-a-number");
    let config = test_config();
    assert_eq!(transaction_amount_string(&t, &config), "not-a-number");
}

#[test]
fn spending_windows_config_is_sorted_deduped_and_nonzero() {
    let mut config = test_config();
    config.tray.spending_windows_days = vec![30, 0, 7, 7];
    assert_eq!(spending_windows_from_config(&config), vec![7, 30]);

    config.tray.spending_windows_days.clear();
    assert_eq!(spending_windows_from_config(&config), vec![7, 30, 90]);
}

#[test]
fn spending_window_summary_uses_reporting_currency_outflows() {
    let mut eur_tx = tx("eur", "2026-02-09T00:00:00+00:00", "-99");
    eur_tx.asset = json!({"type":"currency","iso_code":"EUR"});
    let mut equity_tx = tx("equity", "2026-02-09T00:00:00+00:00", "-999");
    equity_tx.asset = json!({"type":"equity","symbol":"SPY"});
    let mut ignored_tx = tx("ignored", "2026-02-09T00:00:00+00:00", "-30000");
    ignored_tx.annotation = Some(TransactionAnnotationOutput {
        description: None,
        note: None,
        tags: Some(vec!["ignore_spending".to_string()]),
        subtags: None,
        effective_date: None,
    });

    let summaries = summarize_spending_windows(
        &[
            tx("recent", "2026-02-09T00:00:00+00:00", "-10"),
            tx("older", "2026-01-20T00:00:00+00:00", "-20"),
            tx("inflow", "2026-02-09T00:00:00+00:00", "5"),
            eur_tx,
            equity_tx,
            ignored_tx,
            tx("future", "2026-02-12T00:00:00+00:00", "-500"),
        ],
        "USD",
        &[7, 30],
        NaiveDate::from_ymd_opt(2026, 2, 10).expect("valid date"),
    );

    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].days, 7);
    assert_eq!(summaries[0].transaction_count, 1);
    assert_eq!(
        summaries[0].total,
        Decimal::from_str("10").expect("valid decimal")
    );
    assert_eq!(summaries[1].days, 30);
    assert_eq!(summaries[1].transaction_count, 2);
    assert_eq!(
        summaries[1].total,
        Decimal::from_str("30").expect("valid decimal")
    );
}

#[test]
fn net_worth_interval_cycles() {
    assert_eq!(NetWorthInterval::Daily.next(), NetWorthInterval::Weekly);
    assert_eq!(NetWorthInterval::Daily.prev(), NetWorthInterval::Hourly);
    assert_eq!(NetWorthInterval::Full.prev(), NetWorthInterval::Yearly);
}

#[test]
fn net_worth_point_date_uses_date_field() {
    let point = HistoryPoint {
        timestamp: "2026-02-01T14:30:00+00:00".to_string(),
        date: "2026-02-01".to_string(),
        total_value: "1234".to_string(),
        prospective_capital_gains_tax: None,
        percentage_change_from_previous: None,
        change_triggers: None,
    };
    assert_eq!(
        net_worth_point_date(&point),
        Some(NaiveDate::from_ymd_opt(2026, 2, 1).expect("valid date"))
    );
}
