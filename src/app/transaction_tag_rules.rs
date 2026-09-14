//! Transaction tag rules used by the TUI to derive a display tag for a
//! transaction, plus the LLM-assisted regex suggestion used when recording a
//! new rule.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

const TRANSACTION_RULES_FILE: &str = "transaction_rules.jsonl";
const OPENAI_REGEX_SUGGESTION_MODEL_ENV: &str = "KEEPBOOK_REGEX_LLM_MODEL";
const OPENAI_REGEX_SUGGESTION_MODEL_DEFAULT: &str = "gpt-4o-mini";
const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_TIMEOUT_SECS: u64 = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TransactionTagRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) set_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) set_tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) set_subtags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_subtag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) match_amount: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TransactionTagRuleInput<'a> {
    pub(crate) account_id: &'a str,
    pub(crate) account_name: &'a str,
    pub(crate) description: &'a str,
    pub(crate) tag: &'a str,
    pub(crate) subtag: &'a str,
    pub(crate) status: &'a str,
    pub(crate) amount: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledTransactionTagRule {
    pub(crate) tag: Option<String>,
    account_id: Option<Regex>,
    account_name: Option<Regex>,
    description: Option<Regex>,
    tag_matcher: Option<Regex>,
    subtag_matcher: Option<Regex>,
    status: Option<Regex>,
    amount: Option<Regex>,
}

impl CompiledTransactionTagRule {
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
            format!("Invalid tag rule regex [{rule_index}] {field_name}: {trimmed}")
        })?;
        Ok(Some(compiled))
    }

    pub(crate) fn from_rule(rule_index: usize, rule: &TransactionTagRule) -> Result<Self> {
        let tag = rule.set_tags.as_ref().and_then(|tags| {
            tags.iter()
                .map(String::as_str)
                .map(str::trim)
                .find(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });
        let description = rule
            .set_description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let subtags = rule
            .set_subtags
            .as_ref()
            .map(|subtags| subtags.iter().any(|subtag| !subtag.trim().is_empty()))
            .unwrap_or(false);
        if tag.is_none() && description.is_none() && !subtags {
            anyhow::bail!("Invalid tag rule [{rule_index}]: at least one action is required");
        }
        let compiled = Self {
            tag,
            account_id: Self::compile_field(
                rule_index,
                "match_account_id",
                &rule.match_account_id,
            )?,
            account_name: Self::compile_field(
                rule_index,
                "match_account_name",
                &rule.match_account_name,
            )?,
            description: Self::compile_field(
                rule_index,
                "match_description",
                &rule.match_description,
            )?,
            tag_matcher: Self::compile_field(rule_index, "match_tag", &rule.match_tag)?,
            subtag_matcher: Self::compile_field(rule_index, "match_subtag", &rule.match_subtag)?,
            status: Self::compile_field(rule_index, "match_status", &rule.match_status)?,
            amount: Self::compile_field(rule_index, "match_amount", &rule.match_amount)?,
        };
        let has_any_matcher = compiled.account_id.is_some()
            || compiled.account_name.is_some()
            || compiled.description.is_some()
            || compiled.tag_matcher.is_some()
            || compiled.subtag_matcher.is_some()
            || compiled.status.is_some()
            || compiled.amount.is_some();
        if !has_any_matcher {
            anyhow::bail!("Invalid tag rule [{rule_index}]: at least one matcher is required");
        }
        Ok(compiled)
    }

    fn match_field(pattern: &Option<Regex>, value: &str) -> bool {
        pattern
            .as_ref()
            .map(|compiled| compiled.is_match(value))
            .unwrap_or(true)
    }

    fn is_match(&self, input: &TransactionTagRuleInput<'_>) -> bool {
        Self::match_field(&self.account_id, input.account_id)
            && Self::match_field(&self.account_name, input.account_name)
            && Self::match_field(&self.description, input.description)
            && Self::match_field(&self.tag_matcher, input.tag)
            && Self::match_field(&self.subtag_matcher, input.subtag)
            && Self::match_field(&self.status, input.status)
            && Self::match_field(&self.amount, input.amount)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TransactionTagMatcher {
    pub(crate) rules: Vec<CompiledTransactionTagRule>,
}

impl TransactionTagMatcher {
    pub(crate) fn match_tag<'a>(&'a self, input: &TransactionTagRuleInput<'_>) -> Option<&'a str> {
        self.rules
            .iter()
            .find(|rule| rule.tag.is_some() && rule.is_match(input))
            .and_then(|rule| rule.tag.as_deref())
    }
}

pub(crate) fn tag_rules_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TRANSACTION_RULES_FILE)
}

pub(crate) fn load_transaction_tag_rules(
    path: &Path,
) -> Result<(TransactionTagMatcher, Option<String>)> {
    if !path.exists() {
        return Ok((TransactionTagMatcher::default(), None));
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("Unable to open tag rules file: {}", path.display()))?;
    let mut compiled_rules = Vec::new();
    let mut warning_count = 0usize;

    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let raw = line.with_context(|| {
            format!(
                "Unable to read tag rules file line {}: {}",
                line_number + 1,
                path.display()
            )
        })?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: TransactionTagRule = match serde_json::from_str(trimmed) {
            Ok(rule) => rule,
            Err(_) => {
                warning_count += 1;
                continue;
            }
        };
        match CompiledTransactionTagRule::from_rule(compiled_rules.len(), &parsed) {
            Ok(compiled) => compiled_rules.push(compiled),
            Err(_) => warning_count += 1,
        }
    }

    let warning = if warning_count > 0 {
        Some(format!(
            "Skipped {warning_count} invalid tag rules from {}",
            path.display()
        ))
    } else {
        None
    };

    Ok((
        TransactionTagMatcher {
            rules: compiled_rules,
        },
        warning,
    ))
}

pub(crate) fn append_transaction_tag_rule(path: &Path, rule: &TransactionTagRule) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Unable to create tag rules dir: {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| {
            format!(
                "Unable to open tag rules file for append: {}",
                path.display()
            )
        })?;
    let encoded = serde_json::to_string(rule).context("Unable to encode tag rule")?;
    file.write_all(encoded.as_bytes())
        .context("Unable to write tag rule")?;
    file.write_all(b"\n")
        .context("Unable to terminate tag rule record")?;
    Ok(())
}

pub(crate) fn exact_ci_regex_pattern(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("(?i)^{}$", regex::escape(trimmed)))
    }
}

pub(crate) fn fallback_regex_suggestion(description: &str) -> String {
    let words: Vec<String> = description.split_whitespace().map(regex::escape).collect();
    if words.is_empty() {
        "(?i).*".to_string()
    } else {
        format!("(?i)^{}$", words.join("\\s+"))
    }
}

fn sanitize_openai_regex(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let body: Vec<&str> = lines.take_while(|line| !line.starts_with("```")).collect();
        return body.join("\n").trim().to_string();
    }

    trimmed
        .strip_prefix("regex:")
        .or_else(|| trimmed.strip_prefix("REGEX:"))
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

pub(crate) async fn suggest_regex_with_openai(
    tag: &str,
    account_name: &str,
    status: &str,
    amount: &str,
    description: &str,
) -> Result<Option<String>> {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };
    let model = std::env::var(OPENAI_REGEX_SUGGESTION_MODEL_ENV)
        .unwrap_or_else(|_| OPENAI_REGEX_SUGGESTION_MODEL_DEFAULT.to_string());

    let prompt = format!(
        "Tag: {tag}\nAccount: {account_name}\nStatus: {status}\nAmount: {amount}\nDescription: {description}\n\nReturn exactly one Rust regex pattern that matches this description style and avoids overmatching unrelated merchants. No explanation."
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(OPENAI_TIMEOUT_SECS))
        .build()
        .context("Unable to initialize HTTP client for regex suggestion")?;

    let response = client
        .post(OPENAI_CHAT_COMPLETIONS_URL)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "temperature": 0,
            "messages": [
                {
                    "role": "system",
                    "content": "You write precise Rust regex patterns for personal finance transaction descriptions. Respond with only the regex string."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        }))
        .send()
        .await
        .context("OpenAI request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "OpenAI request failed with {status}: {}",
            body.chars().take(200).collect::<String>()
        );
    }

    let value: serde_json::Value = response
        .json()
        .await
        .context("Invalid OpenAI response JSON")?;
    let raw = value
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() {
        return Ok(None);
    }
    let candidate = sanitize_openai_regex(raw);
    if candidate.is_empty() {
        return Ok(None);
    }
    Regex::new(&candidate).context("OpenAI suggested an invalid regex")?;
    Ok(Some(candidate))
}

#[cfg(test)]
#[path = "../../tests/unit/app/transaction_tag_rules_tests.rs"]
mod transaction_tag_rules_tests;
