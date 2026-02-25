use anyhow::{Context, Result};
use regex::Regex;

use crate::config::{IgnoreConfig, TransactionIgnoreRule};

#[derive(Debug, Clone)]
pub struct TransactionIgnoreInput<'a> {
    pub account_id: &'a str,
    pub account_name: &'a str,
    pub connection_id: &'a str,
    pub connection_name: &'a str,
    pub synchronizer: &'a str,
    pub description: &'a str,
    pub status: &'a str,
    pub amount: &'a str,
}

#[derive(Debug, Clone)]
pub struct TransactionIgnoreMatcher {
    rules: Vec<CompiledTransactionIgnoreRule>,
}

impl TransactionIgnoreMatcher {
    pub fn from_config(config: &IgnoreConfig) -> Result<Self> {
        let mut compiled = Vec::with_capacity(config.transaction_rules.len());
        for (idx, rule) in config.transaction_rules.iter().enumerate() {
            compiled.push(CompiledTransactionIgnoreRule::new(idx, rule)?);
        }
        Ok(Self { rules: compiled })
    }

    pub fn is_match(&self, input: &TransactionIgnoreInput<'_>) -> bool {
        self.rules.iter().any(|rule| rule.is_match(input))
    }
}

#[derive(Debug, Clone)]
struct CompiledTransactionIgnoreRule {
    account_id: Option<Regex>,
    account_name: Option<Regex>,
    connection_id: Option<Regex>,
    connection_name: Option<Regex>,
    synchronizer: Option<Regex>,
    description: Option<Regex>,
    status: Option<Regex>,
    amount: Option<Regex>,
}

impl CompiledTransactionIgnoreRule {
    fn compile_field(index: usize, field: &str, value: &Option<String>) -> Result<Option<Regex>> {
        let Some(pattern) = value else {
            return Ok(None);
        };
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let compiled = Regex::new(trimmed).with_context(|| {
            format!("Invalid ignore.transaction_rules[{index}].{field} regex: {trimmed}")
        })?;
        Ok(Some(compiled))
    }

    fn new(index: usize, rule: &TransactionIgnoreRule) -> Result<Self> {
        let compiled = Self {
            account_id: Self::compile_field(index, "account_id", &rule.account_id)?,
            account_name: Self::compile_field(index, "account_name", &rule.account_name)?,
            connection_id: Self::compile_field(index, "connection_id", &rule.connection_id)?,
            connection_name: Self::compile_field(index, "connection_name", &rule.connection_name)?,
            synchronizer: Self::compile_field(index, "synchronizer", &rule.synchronizer)?,
            description: Self::compile_field(index, "description", &rule.description)?,
            status: Self::compile_field(index, "status", &rule.status)?,
            amount: Self::compile_field(index, "amount", &rule.amount)?,
        };

        let has_any_field = compiled.account_id.is_some()
            || compiled.account_name.is_some()
            || compiled.connection_id.is_some()
            || compiled.connection_name.is_some()
            || compiled.synchronizer.is_some()
            || compiled.description.is_some()
            || compiled.status.is_some()
            || compiled.amount.is_some();

        if !has_any_field {
            anyhow::bail!(
                "ignore.transaction_rules[{index}] must specify at least one regex field"
            );
        }

        Ok(compiled)
    }

    fn match_field(pattern: &Option<Regex>, value: &str) -> bool {
        pattern
            .as_ref()
            .map(|re| re.is_match(value))
            .unwrap_or(true)
    }

    fn is_match(&self, input: &TransactionIgnoreInput<'_>) -> bool {
        Self::match_field(&self.account_id, input.account_id)
            && Self::match_field(&self.account_name, input.account_name)
            && Self::match_field(&self.connection_id, input.connection_id)
            && Self::match_field(&self.connection_name, input.connection_name)
            && Self::match_field(&self.synchronizer, input.synchronizer)
            && Self::match_field(&self.description, input.description)
            && Self::match_field(&self.status, input.status)
            && Self::match_field(&self.amount, input.amount)
    }
}
