use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use keepbook::config::ResolvedConfig;
use keepbook::credentials::CredentialConfig;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiRuleTransactionInput {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub timestamp: String,
    pub description: String,
    pub amount: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    #[serde(default)]
    pub ignored_from_spending: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiRuleSuggestionInput {
    pub prompt: String,
    #[serde(default)]
    pub transactions: Vec<AiRuleTransactionInput>,
    #[serde(default)]
    pub existing_categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRuleToolCallOutput {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRuleSuggestionsOutput {
    pub model: String,
    pub selected_transaction_count: usize,
    pub suggestions: Vec<AiRuleToolCallOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

pub async fn suggest_rules(
    config_path: &Path,
    config: &ResolvedConfig,
    input: AiRuleSuggestionInput,
) -> Result<AiRuleSuggestionsOutput> {
    let prompt = input.prompt.trim();
    if prompt.is_empty() {
        bail!("AI rule prompt cannot be empty");
    }
    if input.transactions.is_empty() {
        bail!("Select at least one transaction before requesting rule suggestions");
    }

    let api_key = resolve_openai_api_key(config_path, config).await?;
    let model = normalized_model(config);
    let request_body = openai_request_body(config, &model, prompt, &input)?;
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key.expose_secret())
        .json(&request_body)
        .send()
        .await
        .context("failed to call OpenAI Responses API")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read OpenAI response body")?;
    if !status.is_success() {
        bail!("OpenAI Responses API returned HTTP {status}: {body}");
    }

    let value: Value =
        serde_json::from_str(&body).context("failed to parse OpenAI response JSON")?;
    let suggestions = extract_tool_calls(&value)?;
    let message = extract_output_text(&value);
    Ok(AiRuleSuggestionsOutput {
        model,
        selected_transaction_count: input.transactions.len(),
        suggestions,
        message,
        response_id: value
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

async fn resolve_openai_api_key(
    config_path: &Path,
    config: &ResolvedConfig,
) -> Result<SecretString> {
    if let Some(credentials) = &config.ai.openai.credentials {
        let credential_config: CredentialConfig = credentials
            .clone()
            .try_into()
            .context("failed to parse [ai.openai.credentials]")?;
        let base_dir = config_path.parent();
        let store = credential_config.build_with_base_dir(base_dir);
        for key in ["api_key", "api-key", "token"] {
            if let Some(value) = store
                .get(key)
                .await
                .with_context(|| format!("failed to read OpenAI credential field {key}"))?
            {
                return Ok(value);
            }
        }
        bail!("[ai.openai.credentials] did not expose an api_key field");
    }

    let env_key = config
        .ai
        .openai
        .api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("OPENAI_API_KEY");
    let value = std::env::var(env_key).with_context(|| {
        format!("OpenAI API key not configured; set {env_key} or [ai.openai.credentials]")
    })?;
    if value.trim().is_empty() {
        bail!("{env_key} is set but empty");
    }
    Ok(SecretString::from(value))
}

fn normalized_model(config: &ResolvedConfig) -> String {
    let model = config.ai.openai.model.trim();
    if model.is_empty() {
        "gpt-5.5".to_string()
    } else {
        model.to_string()
    }
}

fn openai_request_body(
    config: &ResolvedConfig,
    model: &str,
    prompt: &str,
    input: &AiRuleSuggestionInput,
) -> Result<Value> {
    let context = json!({
        "user_prompt": prompt,
        "reporting_currency": config.reporting_currency,
        "existing_categories": input.existing_categories,
        "current_ignore_rules": config.ignore.transaction_rules,
        "available_tools": [
            "propose_categorization_rule",
            "propose_ignore_rule",
            "propose_rename_rule"
        ],
        "selected_transactions": input.transactions,
    });
    let context_text = serde_json::to_string_pretty(&context)?;

    Ok(json!({
        "model": model,
        "instructions": system_instructions(),
        "input": format!("Return function tool calls for this keepbook rule-writing request.\n\n{context_text}"),
        "tool_choice": "required",
        "tools": rule_tools(),
    }))
}

fn system_instructions() -> &'static str {
    r#"You help write keepbook transaction automation rules.

Use function tool calls for concrete proposals. Do not claim that a rule has been installed or applied.

Rules must be conservative:
- Prefer regexes that match the selected merchant/title shape without catching unrelated merchants.
- Include account/status/amount constraints when they materially reduce false positives.
- Keep regexes compatible with Rust and JavaScript regex engines.
- Use existing categories when they fit; propose a new category only if needed.
- For ignore rules, target spending exclusions such as transfers, reversals, duplicate cash movements, and non-spending bookkeeping entries.
- For rename rules, preserve useful merchant/account detail and remove volatile IDs or noise.

The keepbook config supports:
- [[ignore.transaction_rules]] with regex fields: account_id, account_name, connection_id, connection_name, synchronizer, description, status, amount.
- Transaction annotation patches for category/subcategory and display description.
"#
}

fn rule_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "propose_categorization_rule",
            "description": "Propose a transaction auto-categorization rule for matching future transactions.",
            "strict": true,
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "description_regex": {"type": "string", "description": "Regex matching transaction descriptions/titles."},
                    "account_name_regex": {"type": "string", "description": "Regex matching account names, or an empty string if not needed."},
                    "status_regex": {"type": "string", "description": "Regex matching statuses, usually ^posted$."},
                    "amount_regex": {"type": "string", "description": "Regex matching amounts, or an empty string if not needed."},
                    "category": {"type": "string"},
                    "subcategory": {"type": "string", "description": "Subcategory or empty string."},
                    "matching_transaction_ids": {"type": "array", "items": {"type": "string"}},
                    "rationale": {"type": "string"}
                },
                "required": [
                    "description_regex",
                    "account_name_regex",
                    "status_regex",
                    "amount_regex",
                    "category",
                    "subcategory",
                    "matching_transaction_ids",
                    "rationale"
                ]
            }
        }),
        json!({
            "type": "function",
            "name": "propose_ignore_rule",
            "description": "Propose an [[ignore.transaction_rules]] entry for transactions that should be excluded from spending/list views.",
            "strict": true,
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "account_id_regex": {"type": "string", "description": "Regex for account_id or empty string."},
                    "account_name_regex": {"type": "string", "description": "Regex for account_name or empty string."},
                    "connection_name_regex": {"type": "string", "description": "Regex for connection_name or empty string."},
                    "description_regex": {"type": "string"},
                    "status_regex": {"type": "string", "description": "Regex matching statuses, usually ^posted$."},
                    "amount_regex": {"type": "string", "description": "Regex matching amounts or empty string."},
                    "matching_transaction_ids": {"type": "array", "items": {"type": "string"}},
                    "rationale": {"type": "string"}
                },
                "required": [
                    "account_id_regex",
                    "account_name_regex",
                    "connection_name_regex",
                    "description_regex",
                    "status_regex",
                    "amount_regex",
                    "matching_transaction_ids",
                    "rationale"
                ]
            }
        }),
        json!({
            "type": "function",
            "name": "propose_rename_rule",
            "description": "Propose a transaction title normalization rule that would produce cleaner display descriptions.",
            "strict": true,
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "description_regex": {"type": "string", "description": "Regex matching noisy source descriptions."},
                    "replacement_description": {"type": "string", "description": "Clean display description to apply."},
                    "matching_transaction_ids": {"type": "array", "items": {"type": "string"}},
                    "rationale": {"type": "string"}
                },
                "required": [
                    "description_regex",
                    "replacement_description",
                    "matching_transaction_ids",
                    "rationale"
                ]
            }
        }),
    ]
}

fn extract_tool_calls(value: &Value) -> Result<Vec<AiRuleToolCallOutput>> {
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("OpenAI response did not include an output array"))?;
    let mut calls = Vec::new();
    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            continue;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let arguments = match item.get("arguments") {
            Some(Value::String(raw)) => serde_json::from_str(raw)
                .with_context(|| format!("failed to parse function arguments for {name}"))?,
            Some(value) => value.clone(),
            None => Value::Object(Default::default()),
        };
        calls.push(AiRuleToolCallOutput {
            name: name.to_string(),
            arguments,
        });
    }
    Ok(calls)
}

fn extract_output_text(value: &Value) -> Option<String> {
    let mut chunks = Vec::new();
    let output = value.get("output")?.as_array()?;
    for item in output {
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in content {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                chunks.push(text.to_string());
            }
        }
    }
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join("\n"))
    }
}
