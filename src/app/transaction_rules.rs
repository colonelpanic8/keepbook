use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::clock::{Clock, SystemClock};
use crate::config::ResolvedConfig;
use crate::models::{Id, TransactionAnnotation, TransactionAnnotationPatch};
use crate::storage::Storage;

use super::maybe_auto_commit;

pub const TRANSACTION_RULES_FILE: &str = "transaction_category_rules.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionAnnotationRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransactionAnnotationRuleInput<'a> {
    pub account_id: &'a str,
    pub account_name: &'a str,
    pub description: &'a str,
    pub status: &'a str,
    pub amount: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct TransactionAnnotationRuleAction<'a> {
    pub rule_index: usize,
    pub category: Option<&'a str>,
    pub description_override: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct CompiledTransactionAnnotationRule {
    rule_index: usize,
    category: Option<String>,
    description_override: Option<String>,
    account_id: Option<Regex>,
    account_name: Option<Regex>,
    description: Option<Regex>,
    status: Option<Regex>,
    amount: Option<Regex>,
}

impl CompiledTransactionAnnotationRule {
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

    pub fn from_rule(rule_index: usize, rule: &TransactionAnnotationRule) -> Result<Self> {
        let category = normalized_nonempty(rule.category.as_deref());
        let description_override = normalized_nonempty(rule.description_override.as_deref());
        if category.is_none() && description_override.is_none() {
            anyhow::bail!(
                "Invalid transaction rule [{rule_index}]: category or description_override is required"
            );
        }

        let compiled = Self {
            rule_index,
            category,
            description_override,
            account_id: Self::compile_field(rule_index, "account_id", &rule.account_id)?,
            account_name: Self::compile_field(rule_index, "account_name", &rule.account_name)?,
            description: Self::compile_field(rule_index, "description", &rule.description)?,
            status: Self::compile_field(rule_index, "status", &rule.status)?,
            amount: Self::compile_field(rule_index, "amount", &rule.amount)?,
        };
        let has_any_matcher = compiled.account_id.is_some()
            || compiled.account_name.is_some()
            || compiled.description.is_some()
            || compiled.status.is_some()
            || compiled.amount.is_some();
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

    fn is_match(&self, input: &TransactionAnnotationRuleInput<'_>) -> bool {
        Self::match_field(&self.account_id, input.account_id)
            && Self::match_field(&self.account_name, input.account_name)
            && Self::match_field(&self.description, input.description)
            && Self::match_field(&self.status, input.status)
            && Self::match_field(&self.amount, input.amount)
    }

    fn action(&self) -> TransactionAnnotationRuleAction<'_> {
        TransactionAnnotationRuleAction {
            rule_index: self.rule_index,
            category: self.category.as_deref(),
            description_override: self.description_override.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransactionAnnotationRuleMatcher {
    rules: Vec<CompiledTransactionAnnotationRule>,
}

impl TransactionAnnotationRuleMatcher {
    #[cfg(test)]
    pub fn from_compiled_rules_for_test(rules: Vec<CompiledTransactionAnnotationRule>) -> Self {
        Self { rules }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.rules
            .iter()
            .filter_map(|rule| rule.category.as_deref())
    }

    pub fn match_annotation<'a>(
        &'a self,
        input: &TransactionAnnotationRuleInput<'_>,
    ) -> Option<TransactionAnnotationRuleAction<'a>> {
        self.rules
            .iter()
            .find(|rule| rule.is_match(input))
            .map(CompiledTransactionAnnotationRule::action)
    }

    pub fn match_category<'a>(
        &'a self,
        input: &TransactionAnnotationRuleInput<'_>,
    ) -> Option<&'a str> {
        self.match_annotation(input)
            .and_then(|action| action.category)
    }

    pub fn match_description_override<'a>(
        &'a self,
        input: &TransactionAnnotationRuleInput<'_>,
    ) -> Option<&'a str> {
        self.match_annotation(input)
            .and_then(|action| action.description_override)
    }
}

#[derive(Debug, Clone)]
pub struct TransactionAnnotationRuleLoad {
    pub matcher: TransactionAnnotationRuleMatcher,
    pub warning: Option<String>,
    pub skipped_invalid_rule_count: usize,
}

pub fn transaction_rules_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TRANSACTION_RULES_FILE)
}

pub fn load_transaction_annotation_rules(path: &Path) -> Result<TransactionAnnotationRuleLoad> {
    if !path.exists() {
        return Ok(TransactionAnnotationRuleLoad {
            matcher: TransactionAnnotationRuleMatcher::default(),
            warning: None,
            skipped_invalid_rule_count: 0,
        });
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("Unable to open transaction rules file: {}", path.display()))?;
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

        let parsed: TransactionAnnotationRule = match serde_json::from_str(trimmed) {
            Ok(rule) => rule,
            Err(_) => {
                warning_count += 1;
                continue;
            }
        };
        match CompiledTransactionAnnotationRule::from_rule(line_number, &parsed) {
            Ok(compiled) => compiled_rules.push(compiled),
            Err(_) => warning_count += 1,
        }
    }

    let warning = if warning_count > 0 {
        Some(format!(
            "Skipped {warning_count} invalid transaction rules from {}",
            path.display()
        ))
    } else {
        None
    };

    Ok(TransactionAnnotationRuleLoad {
        matcher: TransactionAnnotationRuleMatcher {
            rules: compiled_rules,
        },
        warning,
        skipped_invalid_rule_count: warning_count,
    })
}

pub fn append_transaction_annotation_rule(
    path: &Path,
    rule: &TransactionAnnotationRule,
) -> Result<()> {
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

pub fn exact_ci_regex_pattern(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("(?i)^{}$", regex::escape(trimmed)))
    }
}

#[derive(Debug, Clone)]
pub struct ApplyTransactionAnnotationRulesOptions {
    pub rules_path: Option<PathBuf>,
    pub dry_run: bool,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppliedTransactionRuleChange {
    pub rule_index: usize,
    pub account_id: String,
    pub account_name: String,
    pub transaction_id: String,
    pub timestamp: String,
    pub amount: String,
    pub original_description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyTransactionAnnotationRulesOutput {
    pub success: bool,
    pub rules_path: String,
    pub dry_run: bool,
    pub overwrite: bool,
    pub rules_loaded: usize,
    pub skipped_invalid_rule_count: usize,
    pub accounts_processed: usize,
    pub transactions_examined: usize,
    pub transactions_matched: usize,
    pub annotations_written: usize,
    pub changes: Vec<AppliedTransactionRuleChange>,
}

pub async fn apply_transaction_annotation_rules(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    options: ApplyTransactionAnnotationRulesOptions,
) -> Result<ApplyTransactionAnnotationRulesOutput> {
    apply_transaction_annotation_rules_with_clock(storage, config, options, &SystemClock).await
}

pub async fn apply_transaction_annotation_rules_with_clock(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    options: ApplyTransactionAnnotationRulesOptions,
    clock: &dyn Clock,
) -> Result<ApplyTransactionAnnotationRulesOutput> {
    let rules_path = options
        .rules_path
        .clone()
        .unwrap_or_else(|| transaction_rules_path(&config.data_dir));
    let loaded = load_transaction_annotation_rules(&rules_path)?;
    let matcher = loaded.matcher;
    let mut accounts_processed = 0usize;
    let mut transactions_examined = 0usize;
    let mut transactions_matched = 0usize;
    let mut annotations_written = 0usize;
    let mut changes = Vec::new();
    let now = clock.now();

    let accounts = storage.list_accounts().await?;
    for account in accounts {
        accounts_processed += 1;
        let transactions = storage.get_transactions(&account.id).await?;
        let patches = storage
            .get_transaction_annotation_patches(&account.id)
            .await?;
        let mut annotations_by_tx: HashMap<Id, TransactionAnnotation> = HashMap::new();
        for patch in patches {
            let tx_id = patch.transaction_id.clone();
            let ann = annotations_by_tx
                .entry(tx_id.clone())
                .or_insert_with(|| TransactionAnnotation::new(tx_id));
            patch.apply_to(ann);
        }

        let mut patches_to_append = Vec::new();
        for tx in transactions {
            transactions_examined += 1;
            let status = format!("{:?}", tx.status).to_lowercase();
            let Some(action) = matcher.match_annotation(&TransactionAnnotationRuleInput {
                account_id: account.id.as_str(),
                account_name: &account.name,
                description: &tx.description,
                status: &status,
                amount: &tx.amount,
            }) else {
                continue;
            };
            transactions_matched += 1;

            let current = annotations_by_tx
                .get(&tx.id)
                .cloned()
                .unwrap_or_else(|| TransactionAnnotation::new(tx.id.clone()));
            let mut patch = TransactionAnnotationPatch {
                transaction_id: tx.id.clone(),
                timestamp: now,
                description: None,
                note: None,
                category: None,
                tags: None,
                effective_date: None,
            };

            if let Some(category) = action.category {
                let should_set = options.overwrite || current.category.is_none();
                if should_set && current.category.as_deref() != Some(category) {
                    patch.category = Some(Some(category.to_string()));
                }
            }
            if let Some(description_override) = action.description_override {
                let should_set = options.overwrite || current.description.is_none();
                if should_set && current.description.as_deref() != Some(description_override) {
                    patch.description = Some(Some(description_override.to_string()));
                }
            }

            if patch.category.is_none() && patch.description.is_none() {
                continue;
            }

            let mut next = current;
            patch.apply_to(&mut next);
            annotations_by_tx.insert(tx.id.clone(), next);
            changes.push(AppliedTransactionRuleChange {
                rule_index: action.rule_index,
                account_id: account.id.to_string(),
                account_name: account.name.clone(),
                transaction_id: tx.id.to_string(),
                timestamp: tx.timestamp.to_rfc3339(),
                amount: tx.amount.clone(),
                original_description: tx.description.clone(),
                category: patch.category.clone().and_then(|v| v),
                description: patch.description.clone().and_then(|v| v),
            });
            patches_to_append.push(patch);
        }

        if !options.dry_run && !patches_to_append.is_empty() {
            annotations_written += patches_to_append.len();
            storage
                .append_transaction_annotation_patches(&account.id, &patches_to_append)
                .await?;
        }
    }

    if !options.dry_run && annotations_written > 0 {
        maybe_auto_commit(
            config,
            &format!(
                "apply transaction annotation rules from {}",
                rules_path.display()
            ),
        );
    }

    Ok(ApplyTransactionAnnotationRulesOutput {
        success: true,
        rules_path: rules_path.display().to_string(),
        dry_run: options.dry_run,
        overwrite: options.overwrite,
        rules_loaded: matcher.rule_count(),
        skipped_invalid_rule_count: loaded.skipped_invalid_rule_count,
        accounts_processed,
        transactions_examined,
        transactions_matched,
        annotations_written,
        changes,
    })
}

fn normalized_nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::config::ResolvedConfig;
    use crate::models::{
        Account, Asset, Connection, ConnectionConfig, ConnectionState, Transaction,
    };
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};

    fn test_config() -> ResolvedConfig {
        ResolvedConfig::load_or_default(&std::env::temp_dir().join("keepbook.toml")).unwrap()
    }

    #[tokio::test]
    async fn applies_category_and_description_override_without_overwriting_existing() -> Result<()>
    {
        let storage = MemoryStorage::new();
        let config = test_config();
        let account_id = Id::from_string("acct-1");
        let connection_id = Id::from_string("conn-1");
        storage
            .save_connection(&Connection {
                config: ConnectionConfig {
                    name: "Bank".to_string(),
                    synchronizer: "manual".to_string(),
                    credentials: None,
                    balance_staleness: None,
                },
                state: ConnectionState::new_with(
                    connection_id.clone(),
                    Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
                ),
            })
            .await?;
        storage
            .save_account(&Account::new_with(
                account_id.clone(),
                Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
                "Checking",
                connection_id,
            ))
            .await?;
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap());
        let tx = Transaction::new("-4448.88", Asset::currency("USD"), "ACH DEBIT RENT PORTAL")
            .with_id(Id::from_string("tx-1"));
        storage.append_transactions(&account_id, &[tx]).await?;

        let rules = TransactionAnnotationRuleMatcher {
            rules: vec![CompiledTransactionAnnotationRule::from_rule(
                0,
                &TransactionAnnotationRule {
                    category: Some("Rent".to_string()),
                    description_override: Some("Rent - 100 Broderick".to_string()),
                    account_id: None,
                    account_name: exact_ci_regex_pattern("Checking"),
                    description: Some("(?i)rent portal".to_string()),
                    status: None,
                    amount: Some("^-4448\\.88$".to_string()),
                },
            )?],
        };
        let rules_path = std::env::temp_dir().join("keepbook-rent-rule-test.jsonl");
        let rule = &TransactionAnnotationRule {
            category: Some("Rent".to_string()),
            description_override: Some("Rent - 100 Broderick".to_string()),
            account_id: None,
            account_name: exact_ci_regex_pattern("Checking"),
            description: Some("(?i)rent portal".to_string()),
            status: None,
            amount: Some("^-4448\\.88$".to_string()),
        };
        let _ = std::fs::remove_file(&rules_path);
        append_transaction_annotation_rule(&rules_path, rule)?;
        assert_eq!(rules.rule_count(), 1);

        let out = apply_transaction_annotation_rules_with_clock(
            &storage,
            &config,
            ApplyTransactionAnnotationRulesOptions {
                rules_path: Some(rules_path.clone()),
                dry_run: false,
                overwrite: false,
            },
            &clock,
        )
        .await?;
        assert_eq!(out.annotations_written, 1);
        assert_eq!(out.changes[0].category.as_deref(), Some("Rent"));
        assert_eq!(
            out.changes[0].description.as_deref(),
            Some("Rent - 100 Broderick")
        );

        let out_again = apply_transaction_annotation_rules_with_clock(
            &storage,
            &config,
            ApplyTransactionAnnotationRulesOptions {
                rules_path: Some(rules_path.clone()),
                dry_run: false,
                overwrite: false,
            },
            &clock,
        )
        .await?;
        assert_eq!(out_again.annotations_written, 0);
        let _ = std::fs::remove_file(rules_path);
        Ok(())
    }
}
