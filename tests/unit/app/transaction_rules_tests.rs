use chrono::TimeZone;

use super::*;
use crate::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, SpendingConfig, TagsConfig, TrayConfig,
};
use crate::models::{Account, Asset, Connection, ConnectionConfig, Transaction};
use crate::storage::MemoryStorage;

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: TagsConfig {
            aliases: HashMap::new(),
            parents: HashMap::new(),
        },
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

fn rule() -> TransactionRule {
    TransactionRule {
        set_tags: None,
        set_subtags: None,
        set_description: None,
        match_account_id: None,
        match_account_name: None,
        match_description: None,
        match_tag: None,
        match_subtag: None,
        match_status: None,
        match_amount: None,
    }
}

fn input<'a>(description: &'a str, amount: &'a str) -> TransactionRuleInput<'a> {
    TransactionRuleInput {
        account_id: "acct-1",
        account_name: "Checking",
        description,
        tag: "",
        subtag: "",
        status: "posted",
        amount,
    }
}

fn matcher_of(rules: Vec<TransactionRule>) -> TransactionRuleMatcher {
    TransactionRuleMatcher {
        rules: rules
            .iter()
            .enumerate()
            .map(|(index, rule)| CompiledTransactionRule::from_rule(index, rule).unwrap())
            .collect(),
    }
}

#[test]
fn compiling_a_rule_requires_an_action() {
    let mut r = rule();
    r.match_description = Some("coffee".to_string());
    r.set_tags = Some(vec!["  ".to_string(), String::new()]);

    let err = CompiledTransactionRule::from_rule(3, &r).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid transaction rule [3]: at least one action is required"
    );
}

#[test]
fn compiling_a_rule_requires_a_matcher() {
    let mut r = rule();
    r.set_description = Some("Coffee".to_string());
    r.match_description = Some("   ".to_string());

    let err = CompiledTransactionRule::from_rule(1, &r).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid transaction rule [1]: at least one matcher is required"
    );
}

#[test]
fn compiling_a_rule_names_the_field_with_the_bad_regex() {
    let mut r = rule();
    r.set_tags = Some(vec!["food".to_string()]);
    r.match_amount = Some("(".to_string());

    let err = CompiledTransactionRule::from_rule(2, &r).unwrap_err();
    assert_eq!(
        format!("{err:#}").split(':').next().unwrap(),
        "Invalid transaction rule regex [2] match_amount"
    );
}

#[test]
fn the_first_matching_rule_wins() {
    let mut first = rule();
    first.match_description = Some("(?i)coffee".to_string());
    first.set_tags = Some(vec!["coffee".to_string()]);

    let mut second = rule();
    second.match_description = Some("(?i)coffee".to_string());
    second.set_tags = Some(vec!["food".to_string()]);

    let matcher = matcher_of(vec![first, second]);
    let action = matcher
        .match_rule(&input("COFFEE SHOP", "-12.00"))
        .expect("a rule matched");

    assert_eq!(action.set_tags, Some(vec!["coffee".to_string()]));
    assert_eq!(matcher.len(), 2);
}

#[test]
fn every_populated_matcher_has_to_match() {
    let mut r = rule();
    r.match_description = Some("(?i)coffee".to_string());
    r.match_amount = Some("^-".to_string());
    r.set_tags = Some(vec!["coffee".to_string()]);
    let matcher = matcher_of(vec![r]);

    assert!(matcher
        .match_rule(&input("Coffee Shop", "-12.00"))
        .is_some());
    assert!(matcher.match_rule(&input("Coffee Shop", "12.00")).is_none());
    assert!(matcher.match_rule(&input("Bookstore", "-12.00")).is_none());
}

#[test]
fn loading_rules_counts_unparseable_and_invalid_lines_as_warnings() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = transaction_rules_path(dir.path());

    let mut valid = rule();
    valid.match_description = Some("coffee".to_string());
    valid.set_tags = Some(vec!["coffee".to_string()]);
    append_transaction_rule(&path, &valid)?;

    let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
    writeln!(file, "not json")?;
    writeln!(file)?;
    writeln!(file, r#"{{"match_description":"x"}}"#)?;
    drop(file);

    let (rules, matcher, warnings) = load_transaction_rules(&path)?;
    assert_eq!(rules.len(), 1);
    assert_eq!(matcher.len(), 1);
    assert_eq!(warnings, 2);
    Ok(())
}

#[test]
fn loading_rules_from_a_missing_file_yields_nothing() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (rules, matcher, warnings) = load_transaction_rules(&transaction_rules_path(dir.path()))?;
    assert!(rules.is_empty());
    assert!(matcher.is_empty());
    assert_eq!(warnings, 0);
    Ok(())
}

#[test]
fn appending_an_unusable_rule_does_not_create_the_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = transaction_rules_path(dir.path());

    assert!(append_transaction_rule(&path, &rule()).is_err());
    assert!(!path.exists());
    Ok(())
}

struct ApplyFixture {
    _dir: tempfile::TempDir,
    storage: MemoryStorage,
    config: ResolvedConfig,
    account: Account,
    transaction: Transaction,
}

async fn apply_fixture(rules: Vec<TransactionRule>) -> Result<ApplyFixture> {
    let dir = tempfile::tempdir()?;
    let storage = MemoryStorage::new();
    let config = resolved_config(dir.path());

    let connection = Connection::new(ConnectionConfig {
        name: "Test Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let account = Account::new_with(
        Id::from_string("acct-1"),
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        "Checking",
        connection.id().clone(),
    );
    storage.save_account(&account).await?;

    let transaction = Transaction::new("-12.00", Asset::currency("USD"), "Coffee Shop")
        .with_id(Id::from_string("tx-1"))
        .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap());
    storage
        .append_transactions(&account.id, std::slice::from_ref(&transaction))
        .await?;

    let path = transaction_rules_config_path(&config);
    for rule in &rules {
        append_transaction_rule(&path, rule)?;
    }

    Ok(ApplyFixture {
        _dir: dir,
        storage,
        config,
        account,
        transaction,
    })
}

async fn annotation_for(fx: &ApplyFixture) -> Result<TransactionAnnotation> {
    let mut ann = TransactionAnnotation::new(fx.transaction.id.clone());
    for patch in fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .into_iter()
        .filter(|p| p.transaction_id == fx.transaction.id)
    {
        patch.apply_to(&mut ann);
    }
    Ok(ann)
}

fn coffee_rule() -> TransactionRule {
    let mut r = rule();
    r.match_description = Some("(?i)coffee".to_string());
    r.set_tags = Some(vec!["coffee".to_string()]);
    r.set_description = Some("Coffee Shop (rule)".to_string());
    r
}

async fn seed_annotation(fx: &ApplyFixture, patch: TransactionAnnotationPatch) -> Result<()> {
    fx.storage
        .append_transaction_annotation_patches(&fx.account.id, &[patch])
        .await
}

fn patch_for(fx: &ApplyFixture) -> TransactionAnnotationPatch {
    TransactionAnnotationPatch {
        transaction_id: fx.transaction.id.clone(),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 20, 0, 0, 0).unwrap(),
        description: None,
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
        ignore_spending: None,
    }
}

#[tokio::test]
async fn applying_rules_skips_fields_that_already_have_a_value() -> Result<()> {
    let fx = apply_fixture(vec![coffee_rule()]).await?;
    seed_annotation(
        &fx,
        TransactionAnnotationPatch {
            description: Some(Some("Hand written".to_string())),
            tags: Some(Some(vec!["manual".to_string()])),
            ..patch_for(&fx)
        },
    )
    .await?;

    let result = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions::default(),
    )
    .await?;

    assert_eq!(result["matched_count"], serde_json::json!(1));
    assert_eq!(result["updated_count"], serde_json::json!(0));
    assert_eq!(
        result["skipped_existing_action_count"],
        serde_json::json!(1)
    );

    let ann = annotation_for(&fx).await?;
    assert_eq!(ann.description.as_deref(), Some("Hand written"));
    assert_eq!(ann.tags, Some(vec!["manual".to_string()]));
    Ok(())
}

#[tokio::test]
async fn overwrite_replaces_existing_values() -> Result<()> {
    let fx = apply_fixture(vec![coffee_rule()]).await?;
    seed_annotation(
        &fx,
        TransactionAnnotationPatch {
            description: Some(Some("Hand written".to_string())),
            tags: Some(Some(vec!["manual".to_string()])),
            ..patch_for(&fx)
        },
    )
    .await?;

    let result = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            overwrite: true,
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(result["updated_count"], serde_json::json!(1));
    let ann = annotation_for(&fx).await?;
    assert_eq!(ann.description.as_deref(), Some("Coffee Shop (rule)"));
    assert_eq!(ann.tags, Some(vec!["coffee".to_string()]));
    Ok(())
}

#[tokio::test]
async fn a_dry_run_reports_the_update_without_writing_it() -> Result<()> {
    let fx = apply_fixture(vec![coffee_rule()]).await?;

    let result = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            dry_run: true,
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(result["dry_run"], serde_json::json!(true));
    assert_eq!(result["updated_count"], serde_json::json!(1));
    assert_eq!(
        result["updates"][0]["set_tags"],
        serde_json::json!(["coffee"])
    );
    assert!(fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn the_date_window_follows_the_annotated_effective_date() -> Result<()> {
    let fx = apply_fixture(vec![coffee_rule()]).await?;
    seed_annotation(
        &fx,
        TransactionAnnotationPatch {
            effective_date: Some(Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())),
            ..patch_for(&fx)
        },
    )
    .await?;

    let outside = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            dry_run: true,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(outside["matched_count"], serde_json::json!(0));

    let inside = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            start: Some("2026-03-01".to_string()),
            end: Some("2026-03-31".to_string()),
            dry_run: true,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(inside["matched_count"], serde_json::json!(1));
    Ok(())
}

#[tokio::test]
async fn applying_rules_rejects_contradictory_options() -> Result<()> {
    let fx = apply_fixture(vec![coffee_rule()]).await?;

    let err = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            account: Some("acct-1".to_string()),
            connection: Some("Test Bank".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "--account and --connection are mutually exclusive"
    );

    let err = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            start: Some("2026-03-31".to_string()),
            end: Some("2026-03-01".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Start date must be on or before end date");

    let err = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            start: Some("31/03/2026".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Invalid start date: 31/03/2026");
    Ok(())
}

#[tokio::test]
async fn applying_rules_without_any_rules_short_circuits() -> Result<()> {
    let fx = apply_fixture(Vec::new()).await?;

    let result = apply_transaction_rules(
        &fx.storage,
        &fx.config,
        ApplyTransactionRulesOptions {
            start: Some("not-a-date".to_string()),
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(result["rule_count"], serde_json::json!(0));
    assert_eq!(result["matched_count"], serde_json::json!(0));
    Ok(())
}

#[test]
fn tag_values_are_trimmed_and_deduplicated_case_insensitively() {
    let values = vec![
        " Food ".to_string(),
        "food".to_string(),
        String::new(),
        "Coffee".to_string(),
    ];
    assert_eq!(
        normalize_tag_values(Some(values.as_slice())),
        Some(vec!["Food".to_string(), "Coffee".to_string()])
    );

    let blank = vec![" ".to_string()];
    assert_eq!(normalize_tag_values(Some(blank.as_slice())), None);
    assert_eq!(normalize_tag_values(None), None);
}
