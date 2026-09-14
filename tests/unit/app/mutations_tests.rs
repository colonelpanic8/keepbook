use chrono::{NaiveDate, TimeZone};

use super::*;
use crate::clock::FixedClock;
use crate::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, SpendingConfig, TagsConfig, TrayConfig,
};
use crate::models::{FixedIdGenerator, Transaction};
use crate::storage::MemoryStorage;

fn resolved_config(data_dir: &std::path::Path) -> ResolvedConfig {
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

struct Fixture {
    _dir: tempfile::TempDir,
    storage: MemoryStorage,
    config: ResolvedConfig,
    connection: Connection,
    account: Account,
    transaction: Transaction,
}

async fn fixture() -> Result<Fixture> {
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

    Ok(Fixture {
        _dir: dir,
        storage,
        config,
        connection,
        account,
        transaction,
    })
}

async fn annotation_for(fx: &Fixture) -> Result<TransactionAnnotation> {
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

fn targets(fx: &Fixture) -> Vec<(String, String)> {
    vec![(fx.account.id.to_string(), fx.transaction.id.to_string())]
}

#[tokio::test]
async fn remove_connection_rejects_an_unusable_id() -> Result<()> {
    let fx = fixture().await?;
    let err = remove_connection(&fx.storage, &fx.config, "../escape")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Invalid connection id"));
    Ok(())
}

#[tokio::test]
async fn remove_connection_reports_a_missing_connection_without_failing() -> Result<()> {
    let fx = fixture().await?;
    let result = remove_connection(&fx.storage, &fx.config, "nope").await?;
    assert_eq!(result["success"], serde_json::json!(false));
    assert_eq!(result["error"], serde_json::json!("Connection not found"));
    assert!(fx.storage.get_account(&fx.account.id).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn remove_connection_deletes_the_connection_and_its_accounts_once() -> Result<()> {
    let fx = fixture().await?;
    let result = remove_connection(&fx.storage, &fx.config, fx.connection.id().as_str()).await?;

    assert_eq!(result["deleted_accounts"], serde_json::json!(1));
    assert_eq!(
        result["account_ids"],
        serde_json::json!([fx.account.id.to_string()])
    );
    assert!(fx.storage.get_account(&fx.account.id).await?.is_none());
    assert!(fx
        .storage
        .get_connection(fx.connection.id())
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn add_connection_stores_config_and_state() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = MemoryStorage::new();
    let config = resolved_config(dir.path());
    let ids = FixedIdGenerator::new([Id::from_string("conn-1")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    let result = add_connection_with(&storage, &config, "Schwab", "schwab", &ids, &clock).await?;

    assert_eq!(result["connection"]["id"], serde_json::json!("conn-1"));
    let stored = storage
        .get_connection(&Id::from_string("conn-1"))
        .await?
        .expect("connection saved");
    assert_eq!(stored.config.synchronizer, "schwab");
    assert_eq!(stored.config.name, "Schwab");
    Ok(())
}

#[tokio::test]
async fn add_connection_rejects_a_name_that_differs_only_in_case() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("conn-2")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    let err = add_connection_with(&fx.storage, &fx.config, "test bank", "manual", &ids, &clock)
        .await
        .unwrap_err();

    assert_eq!(err.to_string(), "Connection name already exists: test bank");
    assert_eq!(fx.storage.list_connections().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn add_account_links_the_account_to_its_connection() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("acct-2")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    add_account_with(
        &fx.storage,
        &fx.config,
        fx.connection.id().as_str(),
        "Savings",
        vec!["savings".to_string()],
        &ids,
        &clock,
    )
    .await?;

    let account = fx
        .storage
        .get_account(&Id::from_string("acct-2"))
        .await?
        .expect("account saved");
    assert_eq!(account.tags, vec!["savings".to_string()]);
    assert_eq!(account.created_at, clock.now());

    let connection = fx
        .storage
        .get_connection(fx.connection.id())
        .await?
        .expect("connection still present");
    assert!(connection.state.account_ids.contains(&account.id));
    Ok(())
}

#[tokio::test]
async fn add_account_requires_an_existing_connection() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("acct-3")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    let err = add_account_with(
        &fx.storage,
        &fx.config,
        "missing-conn",
        "Savings",
        Vec::new(),
        &ids,
        &clock,
    )
    .await
    .unwrap_err();

    assert_eq!(err.to_string(), "Connection not found");
    assert!(fx
        .storage
        .get_account(&Id::from_string("acct-3"))
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn set_balance_appends_a_snapshot_with_cost_basis() -> Result<()> {
    let fx = fixture().await?;

    let result = set_balance(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "equity:AAPL",
        " 10.5 ",
        Some(" 1200 "),
    )
    .await?;

    assert_eq!(result["balance"]["amount"], serde_json::json!("10.5"));
    assert_eq!(result["balance"]["cost_basis"], serde_json::json!("1200"));

    let snapshot = fx
        .storage
        .get_latest_balance_snapshot(&fx.account.id)
        .await?
        .expect("snapshot appended");
    assert_eq!(snapshot.balances.len(), 1);
    assert_eq!(snapshot.balances[0].asset, Asset::equity("AAPL"));
    assert_eq!(snapshot.balances[0].amount, "10.5");
    assert_eq!(snapshot.balances[0].cost_basis.as_deref(), Some("1200"));
    Ok(())
}

#[tokio::test]
async fn set_balance_rejects_bad_amounts_and_unknown_accounts() -> Result<()> {
    let fx = fixture().await?;

    let err = set_balance(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "USD",
        "   ",
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Amount cannot be empty");

    let err = set_balance(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "USD",
        "abc",
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Invalid amount: abc");

    let err = set_balance(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "USD",
        "1",
        Some("abc"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Invalid cost basis: abc");

    let err = set_balance(&fx.storage, &fx.config, "missing", "USD", "1", None)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Account not found");

    assert!(fx
        .storage
        .get_latest_balance_snapshot(&fx.account.id)
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn set_account_config_sets_then_clears_the_backfill_policy() -> Result<()> {
    let fx = fixture().await?;

    let result = set_account_config(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        Some("carry-earliest"),
        false,
    )
    .await?;
    assert_eq!(
        result["config"]["balance_backfill"],
        serde_json::json!("carry_earliest")
    );
    assert_eq!(
        fx.storage
            .get_account_config(&fx.account.id)?
            .and_then(|c| c.balance_backfill),
        Some(BalanceBackfillPolicy::CarryEarliest)
    );

    let result =
        set_account_config(&fx.storage, &fx.config, fx.account.id.as_str(), None, true).await?;
    assert_eq!(
        result["config"]["balance_backfill"],
        serde_json::Value::Null
    );
    assert_eq!(
        fx.storage
            .get_account_config(&fx.account.id)?
            .and_then(|c| c.balance_backfill),
        None
    );
    Ok(())
}

#[tokio::test]
async fn set_account_config_rejects_conflicting_empty_and_unknown_input() -> Result<()> {
    let fx = fixture().await?;

    let err = set_account_config(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        Some("zero"),
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Cannot use --balance-backfill and --clear-balance-backfill together"
    );

    let err = set_account_config(&fx.storage, &fx.config, fx.account.id.as_str(), None, false)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "No account config fields specified");

    let err = set_account_config(&fx.storage, &fx.config, "nope", Some("zero"), false)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Account not found: nope");

    let err = set_account_config(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        Some("sideways"),
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid balance backfill policy: sideways. Use: none, zero, carry_earliest"
    );
    Ok(())
}

#[tokio::test]
async fn set_transaction_annotation_writes_normalized_fields() -> Result<()> {
    let fx = fixture().await?;

    let result = set_transaction_annotation(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            description: Some("Blue Bottle".to_string()),
            tags: vec!["Food, food".to_string(), " coffee ".to_string()],
            subtags: vec!["cafe".to_string()],
            effective_date: Some("2026-02-11".to_string()),
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(
        result["patch"]["tags"],
        serde_json::json!(["Food", "coffee"])
    );
    assert_eq!(
        result["annotation"]["effective_date"],
        serde_json::json!("2026-02-11")
    );

    let ann = annotation_for(&fx).await?;
    assert_eq!(ann.description.as_deref(), Some("Blue Bottle"));
    assert_eq!(
        ann.tags,
        Some(vec!["Food".to_string(), "coffee".to_string()])
    );
    assert_eq!(ann.subtags, Some(vec!["cafe".to_string()]));
    assert_eq!(
        ann.effective_date,
        Some(NaiveDate::from_ymd_opt(2026, 2, 11).unwrap())
    );
    Ok(())
}

#[tokio::test]
async fn set_transaction_annotation_clears_fields_it_is_asked_to_clear() -> Result<()> {
    let fx = fixture().await?;

    set_transaction_annotation(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            description: Some("Blue Bottle".to_string()),
            tags: vec!["food".to_string()],
            ..Default::default()
        },
    )
    .await?;

    set_transaction_annotation(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            clear_description: true,
            tags_empty: true,
            ..Default::default()
        },
    )
    .await?;

    let ann = annotation_for(&fx).await?;
    assert_eq!(ann.description, None);
    assert_eq!(ann.tags, Some(Vec::new()));
    Ok(())
}

#[tokio::test]
async fn set_transaction_annotation_rejects_conflicting_and_empty_input() -> Result<()> {
    let fx = fixture().await?;
    let account = fx.account.id.as_str();
    let tx = fx.transaction.id.as_str();

    let cases: Vec<(TransactionAnnotationInput, &str)> = vec![
        (
            TransactionAnnotationInput {
                description: Some("x".to_string()),
                clear_description: true,
                ..Default::default()
            },
            "Cannot use --description and --clear-description together",
        ),
        (
            TransactionAnnotationInput {
                note: Some("x".to_string()),
                clear_note: true,
                ..Default::default()
            },
            "Cannot use --note and --clear-note together",
        ),
        (
            TransactionAnnotationInput {
                tags_empty: true,
                clear_tags: true,
                ..Default::default()
            },
            "Cannot use --clear-tags with --tag/--tags-empty",
        ),
        (
            TransactionAnnotationInput {
                subtags: vec!["x".to_string()],
                clear_subtags: true,
                ..Default::default()
            },
            "Cannot use --clear-subtags with --subtag/--subtags-empty",
        ),
        (
            TransactionAnnotationInput {
                effective_date: Some("2026-02-11".to_string()),
                clear_effective_date: true,
                ..Default::default()
            },
            "Cannot use --effective-date and --clear-effective-date together",
        ),
        (
            TransactionAnnotationInput::default(),
            "No annotation fields specified",
        ),
        (
            TransactionAnnotationInput {
                effective_date: Some("11/02/2026".to_string()),
                ..Default::default()
            },
            "Invalid effective date: 11/02/2026",
        ),
    ];

    for (input, expected) in cases {
        let err = set_transaction_annotation(&fx.storage, &fx.config, account, tx, input)
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), expected);
    }

    assert!(fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn set_transaction_annotation_requires_the_transaction_to_be_in_the_account() -> Result<()> {
    let fx = fixture().await?;
    let input = || TransactionAnnotationInput {
        note: Some("hi".to_string()),
        ..Default::default()
    };

    let err = set_transaction_annotation(
        &fx.storage,
        &fx.config,
        "missing-account",
        fx.transaction.id.as_str(),
        input(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Account not found");

    let err = set_transaction_annotation(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "tx-missing",
        input(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Transaction not found for account");
    Ok(())
}

#[tokio::test]
async fn set_transaction_tags_deduplicates_repeated_targets() -> Result<()> {
    let fx = fixture().await?;
    let mut repeated = targets(&fx);
    repeated.extend(targets(&fx));

    let result = set_transaction_tags(
        &fx.storage,
        &fx.config,
        repeated,
        vec!["Food,food".to_string(), "coffee".to_string()],
        false,
    )
    .await?;

    assert_eq!(result["updated_count"], serde_json::json!(1));
    assert_eq!(result["tags"], serde_json::json!(["Food", "coffee"]));
    assert_eq!(
        annotation_for(&fx).await?.tags,
        Some(vec!["Food".to_string(), "coffee".to_string()])
    );
    Ok(())
}

#[tokio::test]
async fn set_transaction_subtags_clears_the_override_when_asked() -> Result<()> {
    let fx = fixture().await?;

    set_transaction_subtags(
        &fx.storage,
        &fx.config,
        targets(&fx),
        vec!["cafe".to_string()],
        false,
    )
    .await?;
    set_transaction_subtags(&fx.storage, &fx.config, targets(&fx), Vec::new(), true).await?;

    assert_eq!(annotation_for(&fx).await?.subtags, None);
    Ok(())
}

#[tokio::test]
async fn set_transaction_label_list_rejects_conflicting_and_unknown_targets() -> Result<()> {
    let fx = fixture().await?;

    let err = set_transaction_tags(
        &fx.storage,
        &fx.config,
        targets(&fx),
        vec!["food".to_string()],
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Cannot set and clear tags together");

    let err = set_transaction_tags(&fx.storage, &fx.config, targets(&fx), Vec::new(), false)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "No tags change specified");

    let err = set_transaction_subtags(
        &fx.storage,
        &fx.config,
        Vec::new(),
        vec!["cafe".to_string()],
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "No transactions specified");

    let err = set_transaction_tags(
        &fx.storage,
        &fx.config,
        vec![(fx.account.id.to_string(), "tx-missing".to_string())],
        vec!["food".to_string()],
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Transaction not found for account: tx-missing"
    );

    assert!(fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn set_transaction_ignore_requires_targets_that_exist() -> Result<()> {
    let fx = fixture().await?;

    let err = set_transaction_ignore(&fx.storage, &fx.config, Vec::new(), true)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "No transactions specified");

    let err = set_transaction_ignore(
        &fx.storage,
        &fx.config,
        vec![("missing-account".to_string(), fx.transaction.id.to_string())],
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Account not found");
    Ok(())
}

#[tokio::test]
async fn set_transaction_ignore_leaves_ordinary_tags_alone_when_un_ignoring() -> Result<()> {
    let fx = fixture().await?;

    set_transaction_tags(
        &fx.storage,
        &fx.config,
        targets(&fx),
        vec!["food".to_string()],
        false,
    )
    .await?;
    set_transaction_ignore(&fx.storage, &fx.config, targets(&fx), true).await?;
    set_transaction_ignore(&fx.storage, &fx.config, targets(&fx), false).await?;

    let ann = annotation_for(&fx).await?;
    assert_eq!(ann.ignore_spending, None);
    assert_eq!(ann.tags, Some(vec!["food".to_string()]));
    Ok(())
}

#[tokio::test]
async fn propose_transaction_edit_stores_a_pending_proposal() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("prop-1")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    let result = propose_transaction_edit_with(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            description: Some("Blue Bottle".to_string()),
            tags: vec!["Food,food".to_string()],
            ..Default::default()
        },
        &ids,
        &clock,
    )
    .await?;

    assert_eq!(result["proposal"]["id"], serde_json::json!("prop-1"));
    assert_eq!(result["proposal"]["status"], serde_json::json!("pending"));
    assert_eq!(
        result["proposal"]["created_at"],
        serde_json::json!(clock.now().to_rfc3339())
    );
    assert_eq!(
        result["proposal"]["patch"]["tags"],
        serde_json::json!(["Food"])
    );

    // A proposal is not an annotation until it is approved.
    assert!(fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .is_empty());

    let listed = list_proposed_transaction_edits(&fx.storage, false).await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].account_name, "Checking");
    assert_eq!(listed[0].transaction_description, "Coffee Shop");
    Ok(())
}

#[tokio::test]
async fn propose_transaction_edit_validates_ids_and_input() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("prop-2")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    let err = propose_transaction_edit_with(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput::default(),
        &ids,
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "No annotation fields specified");

    let err = propose_transaction_edit_with(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        "tx-missing",
        TransactionAnnotationInput {
            note: Some("hi".to_string()),
            ..Default::default()
        },
        &ids,
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "Transaction not found for account");

    assert!(fx
        .storage
        .get_proposed_transaction_edits()
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn approving_a_proposal_applies_it_and_closes_it() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("prop-3")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    propose_transaction_edit_with(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            description: Some("Blue Bottle".to_string()),
            ..Default::default()
        },
        &ids,
        &clock,
    )
    .await?;

    let result = approve_proposed_transaction_edit(&fx.storage, &fx.config, "prop-3").await?;
    assert_eq!(result["proposal"]["status"], serde_json::json!("approved"));
    assert_eq!(
        annotation_for(&fx).await?.description.as_deref(),
        Some("Blue Bottle")
    );

    let err = reject_proposed_transaction_edit(&fx.storage, &fx.config, "prop-3")
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Proposed transaction edit is already decided"
    );

    assert!(list_proposed_transaction_edits(&fx.storage, false)
        .await?
        .is_empty());
    assert_eq!(
        list_proposed_transaction_edits(&fx.storage, true)
            .await?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn rejecting_and_removing_a_proposal_leave_the_annotation_untouched() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("prop-4"), Id::from_string("prop-5")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    for _ in 0..2 {
        propose_transaction_edit_with(
            &fx.storage,
            &fx.config,
            fx.account.id.as_str(),
            fx.transaction.id.as_str(),
            TransactionAnnotationInput {
                note: Some("check this".to_string()),
                ..Default::default()
            },
            &ids,
            &clock,
        )
        .await?;
    }

    let rejected = reject_proposed_transaction_edit(&fx.storage, &fx.config, "prop-4").await?;
    assert_eq!(
        rejected["proposal"]["status"],
        serde_json::json!("rejected")
    );
    let removed = remove_proposed_transaction_edit(&fx.storage, &fx.config, "prop-5").await?;
    assert_eq!(removed["proposal"]["status"], serde_json::json!("removed"));

    assert!(fx
        .storage
        .get_transaction_annotation_patches(&fx.account.id)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn deciding_an_unknown_proposal_is_an_error() -> Result<()> {
    let fx = fixture().await?;

    let err = approve_proposed_transaction_edit(&fx.storage, &fx.config, "../escape")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Invalid proposal id"));

    let err = approve_proposed_transaction_edit(&fx.storage, &fx.config, "prop-missing")
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Proposed transaction edit not found");
    Ok(())
}

#[tokio::test]
async fn list_proposed_transaction_edits_skips_proposals_whose_target_is_gone() -> Result<()> {
    let fx = fixture().await?;
    let ids = FixedIdGenerator::new([Id::from_string("prop-6")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap());

    propose_transaction_edit_with(
        &fx.storage,
        &fx.config,
        fx.account.id.as_str(),
        fx.transaction.id.as_str(),
        TransactionAnnotationInput {
            note: Some("check this".to_string()),
            ..Default::default()
        },
        &ids,
        &clock,
    )
    .await?;

    fx.storage.delete_account(&fx.account.id).await?;

    assert!(list_proposed_transaction_edits(&fx.storage, true)
        .await?
        .is_empty());
    Ok(())
}

#[test]
fn parse_asset_understands_every_supported_prefix() {
    assert_eq!(parse_asset("usd").unwrap(), Asset::currency("usd"));
    assert_eq!(parse_asset("equity:AAPL").unwrap(), Asset::equity("AAPL"));
    assert_eq!(parse_asset("crypto:BTC").unwrap(), Asset::crypto("BTC"));
    assert_eq!(
        parse_asset("currency: EUR ").unwrap(),
        Asset::currency("EUR")
    );
    assert_eq!(
        parse_asset("value:EUR:Art").unwrap(),
        Asset::manual_value("Art", "EUR")
    );
}

#[test]
fn parse_asset_rejects_empty_and_incomplete_values() {
    assert_eq!(
        parse_asset("   ").unwrap_err().to_string(),
        "Asset string cannot be empty"
    );
    assert_eq!(
        parse_asset("equity:").unwrap_err().to_string(),
        "Asset value missing for prefix 'equity'"
    );
    assert_eq!(
        parse_asset("value:EUR:").unwrap_err().to_string(),
        "Manual value asset must be value:<name> or value:<currency>:<name>"
    );
}
