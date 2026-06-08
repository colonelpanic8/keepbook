use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::ResolvedConfig;
use crate::models::{Id, TransactionAnnotation, TransactionAnnotationPatch};
use crate::storage::{find_account, find_connection, Storage};

use super::classification::{
    effective_transaction_subtags, effective_transaction_tags, provider_virtual_tag_hierarchy,
};
use super::maybe_auto_commit;

const TRANSACTION_RULES_FILE: &str = "transaction_rules.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_subtags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_subtag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_amount: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransactionRuleInput<'a> {
    pub account_id: &'a str,
    pub account_name: &'a str,
    pub description: &'a str,
    pub tag: &'a str,
    pub subtag: &'a str,
    pub status: &'a str,
    pub amount: &'a str,
}

#[derive(Debug, Clone)]
struct TransactionRuleAction {
    set_tags: Option<Vec<String>>,
    set_subtags: Option<Vec<String>>,
    set_description: Option<String>,
}

impl TransactionRuleAction {
    fn is_empty(&self) -> bool {
        self.set_tags.is_none() && self.set_subtags.is_none() && self.set_description.is_none()
    }
}

#[derive(Debug, Clone)]
struct CompiledTransactionRule {
    action: TransactionRuleAction,
    match_account_id: Option<Regex>,
    match_account_name: Option<Regex>,
    match_description: Option<Regex>,
    match_tag: Option<Regex>,
    match_subtag: Option<Regex>,
    match_status: Option<Regex>,
    match_amount: Option<Regex>,
}

impl CompiledTransactionRule {
    fn compile_field(
        rule_index: usize,
        field_name: &str,
        value: &Option<String>,
    ) -> Result<Option<Regex>> {
        let Some(raw_pattern) = value else {
            return Ok(None);
        };
        let trimmed = raw_pattern.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let compiled = Regex::new(trimmed).with_context(|| {
            format!("Invalid transaction rule regex [{rule_index}] {field_name}: {trimmed}")
        })?;
        Ok(Some(compiled))
    }

    fn from_rule(rule_index: usize, rule: &TransactionRule) -> Result<Self> {
        let action = TransactionRuleAction {
            set_tags: normalize_tag_values(rule.set_tags.as_deref()),
            set_subtags: normalize_tag_values(rule.set_subtags.as_deref()),
            set_description: normalize_action_value(rule.set_description.as_deref()),
        };
        if action.is_empty() {
            anyhow::bail!(
                "Invalid transaction rule [{rule_index}]: at least one action is required"
            );
        }
        let compiled = Self {
            action,
            match_account_id: Self::compile_field(
                rule_index,
                "match_account_id",
                &rule.match_account_id,
            )?,
            match_account_name: Self::compile_field(
                rule_index,
                "match_account_name",
                &rule.match_account_name,
            )?,
            match_description: Self::compile_field(
                rule_index,
                "match_description",
                &rule.match_description,
            )?,
            match_tag: Self::compile_field(rule_index, "match_tag", &rule.match_tag)?,
            match_subtag: Self::compile_field(rule_index, "match_subtag", &rule.match_subtag)?,
            match_status: Self::compile_field(rule_index, "match_status", &rule.match_status)?,
            match_amount: Self::compile_field(rule_index, "match_amount", &rule.match_amount)?,
        };
        let has_any_matcher = compiled.match_account_id.is_some()
            || compiled.match_account_name.is_some()
            || compiled.match_description.is_some()
            || compiled.match_tag.is_some()
            || compiled.match_subtag.is_some()
            || compiled.match_status.is_some()
            || compiled.match_amount.is_some();
        if !has_any_matcher {
            anyhow::bail!(
                "Invalid transaction rule [{rule_index}]: at least one matcher is required"
            );
        }
        Ok(compiled)
    }

    fn match_field(pattern: &Option<Regex>, value: &str) -> bool {
        pattern
            .as_ref()
            .map(|compiled| compiled.is_match(value))
            .unwrap_or(true)
    }

    fn is_match(&self, input: &TransactionRuleInput<'_>) -> bool {
        Self::match_field(&self.match_account_id, input.account_id)
            && Self::match_field(&self.match_account_name, input.account_name)
            && Self::match_field(&self.match_description, input.description)
            && Self::match_field(&self.match_tag, input.tag)
            && Self::match_field(&self.match_subtag, input.subtag)
            && Self::match_field(&self.match_status, input.status)
            && Self::match_field(&self.match_amount, input.amount)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransactionRuleMatcher {
    rules: Vec<CompiledTransactionRule>,
}

impl TransactionRuleMatcher {
    fn match_rule<'a>(
        &'a self,
        input: &TransactionRuleInput<'_>,
    ) -> Option<&'a TransactionRuleAction> {
        self.rules
            .iter()
            .find(|rule| rule.is_match(input))
            .map(|rule| &rule.action)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApplyTransactionRulesOptions {
    pub start: Option<String>,
    pub end: Option<String>,
    pub account: Option<String>,
    pub connection: Option<String>,
    pub overwrite: bool,
    pub dry_run: bool,
}

pub fn transaction_rules_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TRANSACTION_RULES_FILE)
}

pub fn transaction_rules_config_path(config: &ResolvedConfig) -> PathBuf {
    transaction_rules_path(&config.data_dir)
}

pub fn load_transaction_rules(
    path: &Path,
) -> Result<(Vec<TransactionRule>, TransactionRuleMatcher, usize)> {
    if !path.exists() {
        return Ok((Vec::new(), TransactionRuleMatcher::default(), 0));
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("Unable to open transaction rules file: {}", path.display()))?;
    let mut parsed_rules = Vec::new();
    let mut compiled_rules = Vec::new();
    let mut warning_count = 0usize;

    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let raw = line.with_context(|| {
            format!(
                "Unable to read transaction rules file line {}: {}",
                line_number + 1,
                path.display()
            )
        })?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: TransactionRule = match serde_json::from_str(trimmed) {
            Ok(rule) => rule,
            Err(_) => {
                warning_count += 1;
                continue;
            }
        };
        match CompiledTransactionRule::from_rule(compiled_rules.len(), &parsed) {
            Ok(compiled) => {
                parsed_rules.push(parsed);
                compiled_rules.push(compiled);
            }
            Err(_) => warning_count += 1,
        }
    }

    Ok((
        parsed_rules,
        TransactionRuleMatcher {
            rules: compiled_rules,
        },
        warning_count,
    ))
}

pub fn append_transaction_rule(path: &Path, rule: &TransactionRule) -> Result<()> {
    CompiledTransactionRule::from_rule(0, rule)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Unable to create transaction rules dir: {}",
                parent.display()
            )
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| {
            format!(
                "Unable to open transaction rules file for append: {}",
                path.display()
            )
        })?;
    let encoded = serde_json::to_string(rule).context("Unable to encode transaction rule")?;
    file.write_all(encoded.as_bytes())
        .context("Unable to write transaction rule")?;
    file.write_all(b"\n")
        .context("Unable to terminate transaction rule record")?;
    Ok(())
}

pub async fn add_transaction_rule(
    config: &ResolvedConfig,
    rule: TransactionRule,
) -> Result<serde_json::Value> {
    let path = transaction_rules_config_path(config);
    append_transaction_rule(&path, &rule)?;
    maybe_auto_commit(config, "add transaction rule");
    Ok(serde_json::json!({
        "success": true,
        "path": path,
        "rule": rule,
    }))
}

pub fn list_transaction_rules(config: &ResolvedConfig) -> Result<serde_json::Value> {
    let path = transaction_rules_config_path(config);
    let (rules, matcher, invalid_rule_count) = load_transaction_rules(&path)?;
    Ok(serde_json::json!({
        "path": path,
        "rule_count": matcher.len(),
        "invalid_rule_count": invalid_rule_count,
        "rules": rules,
    }))
}

pub async fn apply_transaction_rules(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    opts: ApplyTransactionRulesOptions,
) -> Result<serde_json::Value> {
    apply_transaction_rules_with_auto_commit(storage, config, opts, true).await
}

pub(crate) async fn apply_transaction_rules_without_auto_commit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    opts: ApplyTransactionRulesOptions,
) -> Result<serde_json::Value> {
    apply_transaction_rules_with_auto_commit(storage, config, opts, false).await
}

async fn apply_transaction_rules_with_auto_commit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    opts: ApplyTransactionRulesOptions,
    auto_commit: bool,
) -> Result<serde_json::Value> {
    if opts.account.is_some() && opts.connection.is_some() {
        anyhow::bail!("--account and --connection are mutually exclusive");
    }

    let path = transaction_rules_config_path(config);
    let (_, matcher, invalid_rule_count) = load_transaction_rules(&path)?;
    if matcher.is_empty() {
        return Ok(serde_json::json!({
            "success": true,
            "path": path,
            "dry_run": opts.dry_run,
            "rule_count": 0,
            "invalid_rule_count": invalid_rule_count,
            "matched_count": 0,
            "updated_count": 0,
            "skipped_existing_action_count": 0,
        }));
    }

    let start_date = parse_date_opt("start", &opts.start)?;
    let end_date = parse_date_opt("end", &opts.end)?;
    if let (Some(start), Some(end)) = (start_date, end_date) {
        if start > end {
            anyhow::bail!("Start date must be on or before end date");
        }
    }

    let account_ids =
        resolve_account_ids(storage, opts.account.as_deref(), opts.connection.as_deref()).await?;
    let accounts_by_id = storage
        .list_accounts()
        .await?
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect::<HashMap<_, _>>();

    let mut matched_count = 0usize;
    let mut updated_count = 0usize;
    let mut skipped_existing_action_count = 0usize;
    let mut updates = Vec::new();
    let timestamp = Utc::now();

    for account_id in account_ids {
        let Some(account) = accounts_by_id.get(&account_id) else {
            continue;
        };
        let transactions = storage.get_transactions(&account_id).await?;
        let annotations = materialize_annotations(
            storage
                .get_transaction_annotation_patches(&account_id)
                .await?,
        );
        let mut seen_transaction_ids = HashSet::new();
        let mut patches = Vec::new();

        for tx in transactions {
            if !seen_transaction_ids.insert(tx.id.clone()) {
                continue;
            }
            let existing_annotation = annotations.get(&tx.id);
            let tx_date = existing_annotation
                .and_then(|annotation| annotation.effective_date)
                .unwrap_or_else(|| tx.timestamp.date_naive());
            if start_date.is_some_and(|start| tx_date < start)
                || end_date.is_some_and(|end| tx_date > end)
            {
                continue;
            }
            let provider_hierarchy = provider_virtual_tag_hierarchy(
                tx.standardized_metadata.as_ref(),
                &tx.synchronizer_data,
                &config.tags,
            );
            let match_tags =
                effective_transaction_tags(existing_annotation, &provider_hierarchy, &config.tags);
            let match_subtags = effective_transaction_subtags(
                existing_annotation,
                &provider_hierarchy,
                &config.tags,
            );
            let match_tag = match_tags.first().map(String::as_str).unwrap_or("");
            let match_subtag = match_subtags.first().map(String::as_str).unwrap_or("");
            let existing_description = existing_annotation
                .and_then(|annotation| annotation.description.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let existing_tags = existing_annotation
                .and_then(|annotation| annotation.tags.as_ref())
                .filter(|tags| !tags.is_empty());
            let existing_subtags = existing_annotation
                .and_then(|annotation| annotation.subtags.as_ref())
                .filter(|subtags| !subtags.is_empty());
            let status = format!("{:?}", tx.status).to_lowercase();
            let Some(action) = matcher.match_rule(&TransactionRuleInput {
                account_id: account.id.as_str(),
                account_name: &account.name,
                description: &tx.description,
                tag: match_tag,
                subtag: match_subtag,
                status: &status,
                amount: &tx.amount,
            }) else {
                continue;
            };

            matched_count += 1;

            let description_update = if action.set_description.is_some()
                && (opts.overwrite || existing_description.is_none())
            {
                action.set_description.clone()
            } else {
                None
            };
            let tags_update =
                if action.set_tags.is_some() && (opts.overwrite || existing_tags.is_none()) {
                    action.set_tags.clone()
                } else {
                    None
                };
            let subtags_update =
                if action.set_subtags.is_some() && (opts.overwrite || existing_subtags.is_none()) {
                    action.set_subtags.clone()
                } else {
                    None
                };

            if description_update.is_none() && tags_update.is_none() && subtags_update.is_none() {
                skipped_existing_action_count += 1;
                continue;
            }

            updates.push(serde_json::json!({
                "account_id": account.id.to_string(),
                "account_name": account.name.clone(),
                "transaction_id": tx.id.to_string(),
                "description": tx.description.clone(),
                "amount": tx.amount.clone(),
                "set_description": description_update,
                "set_tags": tags_update,
                "set_subtags": subtags_update,
                "previous_tag": match_tag,
                "previous_subtag": match_subtag,
                "previous_description": existing_description,
                "previous_tags": existing_tags,
                "previous_subtags": existing_subtags,
            }));
            patches.push(TransactionAnnotationPatch {
                transaction_id: tx.id,
                timestamp,
                description: description_update.map(Some),
                note: None,
                tags: tags_update.map(Some),
                subtags: subtags_update.map(Some),
                effective_date: None,
            });
        }

        updated_count += patches.len();
        if !opts.dry_run && !patches.is_empty() {
            storage
                .append_transaction_annotation_patches(&account_id, &patches)
                .await?;
        }
    }

    if auto_commit && !opts.dry_run && updated_count > 0 {
        maybe_auto_commit(
            config,
            &format!("apply transaction rules to {updated_count} transactions"),
        );
    }

    Ok(serde_json::json!({
        "success": true,
        "path": path,
        "dry_run": opts.dry_run,
        "rule_count": matcher.len(),
        "invalid_rule_count": invalid_rule_count,
        "matched_count": matched_count,
        "updated_count": updated_count,
        "skipped_existing_action_count": skipped_existing_action_count,
        "updates": updates,
    }))
}

async fn resolve_account_ids(
    storage: &dyn Storage,
    account: Option<&str>,
    connection: Option<&str>,
) -> Result<Vec<Id>> {
    if let Some(id_or_name) = account {
        let account = find_account(storage, id_or_name)
            .await?
            .context(format!("Account not found: {id_or_name}"))?;
        return Ok(vec![account.id]);
    }

    if let Some(id_or_name) = connection {
        let connection = find_connection(storage, id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let accounts = storage.list_accounts().await?;
        return Ok(accounts
            .into_iter()
            .filter(|account| account.connection_id == *connection.id())
            .map(|account| account.id)
            .collect());
    }

    Ok(storage
        .list_accounts()
        .await?
        .into_iter()
        .map(|account| account.id)
        .collect())
}

fn materialize_annotations(
    patches: Vec<TransactionAnnotationPatch>,
) -> HashMap<Id, TransactionAnnotation> {
    let mut annotations_by_tx: HashMap<Id, TransactionAnnotation> = HashMap::new();
    for patch in patches {
        let tx_id = patch.transaction_id.clone();
        let ann = annotations_by_tx
            .entry(tx_id.clone())
            .or_insert_with(|| TransactionAnnotation::new(tx_id));
        patch.apply_to(ann);
    }
    annotations_by_tx
}

fn parse_date_opt(label: &str, s: &Option<String>) -> Result<Option<NaiveDate>> {
    s.as_ref()
        .map(|v| {
            NaiveDate::parse_from_str(v, "%Y-%m-%d")
                .with_context(|| format!("Invalid {label} date: {v}"))
        })
        .transpose()
}

fn normalize_action_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_tag_values(values: Option<&[String]>) -> Option<Vec<String>> {
    let tags = values?
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .fold(Vec::<String>::new(), |mut acc, tag| {
            if !acc
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&tag))
            {
                acc.push(tag);
            }
            acc
        });
    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}
