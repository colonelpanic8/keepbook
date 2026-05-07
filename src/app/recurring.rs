use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::storage::Storage;

use super::list::list_transactions;
use super::{
    RecurringTransactionAmountOutput, RecurringTransactionOccurrenceOutput,
    RecurringTransactionOutput, RecurringTransactionsOptions, TransactionOutput,
};

const DEFAULT_RECURRING_START: &str = "1900-01-01";

#[derive(Debug, Clone)]
struct CandidateTransaction {
    id: String,
    account_id: String,
    account_name: String,
    date: NaiveDate,
    description: String,
    display_name: String,
    normalized_name: String,
    compact_name: String,
    tokens: HashSet<String>,
    amount: Decimal,
    amount_abs: Decimal,
    amount_raw: String,
    asset: serde_json::Value,
    asset_key: String,
    direction: &'static str,
}

#[derive(Debug, Clone)]
struct CadenceFit {
    cadence: &'static str,
    score: f64,
    interval_count: u32,
}

struct CandidateEvaluation {
    output: RecurringTransactionOutput,
    confidence: f64,
    transaction_ids: HashSet<String>,
}

pub async fn list_recurring_transactions(
    storage: &dyn Storage,
    options: RecurringTransactionsOptions,
    config: &ResolvedConfig,
) -> Result<Vec<RecurringTransactionOutput>> {
    let start = Some(
        options
            .start
            .clone()
            .unwrap_or_else(|| DEFAULT_RECURRING_START.to_string()),
    );
    let transactions = list_transactions(
        storage,
        start,
        options.end.clone(),
        false,
        !options.include_ignored,
        config,
    )
    .await?;

    let candidates = transaction_candidates(transactions)?;
    let mut evaluations = evaluate_recurring_candidates(&candidates);
    evaluations.retain(|candidate| {
        candidate.confidence >= options.min_confidence
            && (options.include_possible || candidate.output.status == "confirmed")
    });
    evaluations.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.output.occurrence_count.cmp(&a.output.occurrence_count))
            .then_with(|| a.output.name.cmp(&b.output.name))
    });

    let mut accepted: Vec<CandidateEvaluation> = Vec::new();
    for evaluation in evaluations {
        let duplicate = accepted.iter().any(|existing| {
            (existing.output.normalized_name == evaluation.output.normalized_name
                && existing.output.cadence == evaluation.output.cadence
                && (evaluation
                    .transaction_ids
                    .is_subset(&existing.transaction_ids)
                    || existing.transaction_ids == evaluation.transaction_ids))
                || (existing.output.normalized_name == evaluation.output.normalized_name
                    && existing.output.amount.typical == evaluation.output.amount.typical
                    && !existing
                        .transaction_ids
                        .is_disjoint(&evaluation.transaction_ids))
        });
        if !duplicate {
            accepted.push(evaluation);
        }
    }

    Ok(accepted
        .into_iter()
        .map(|candidate| candidate.output)
        .collect())
}

fn transaction_candidates(
    transactions: Vec<TransactionOutput>,
) -> Result<Vec<CandidateTransaction>> {
    let mut out = Vec::new();
    for tx in transactions {
        if tx.status != "posted" {
            continue;
        }

        let amount = Decimal::from_str(&tx.amount)
            .with_context(|| format!("Invalid transaction amount: {}", tx.amount))?;
        if amount.is_zero() {
            continue;
        }

        let date = tx
            .timestamp
            .get(0..10)
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            .with_context(|| format!("Invalid transaction timestamp: {}", tx.timestamp))?;
        let display_name = tx
            .standardized_metadata
            .as_ref()
            .and_then(|metadata| metadata.merchant_name.as_deref())
            .or_else(|| {
                tx.annotation
                    .as_ref()
                    .and_then(|annotation| annotation.description.as_deref())
            })
            .unwrap_or(&tx.description)
            .trim()
            .to_string();
        let normalized_name = normalize_recurring_name(&display_name);
        if normalized_name.is_empty() {
            continue;
        }
        let compact_name = normalized_name.replace(' ', "");
        let tokens = normalized_name
            .split_whitespace()
            .map(|token| token.to_string())
            .collect::<HashSet<_>>();
        let direction = if amount.is_sign_negative() {
            "outflow"
        } else {
            "inflow"
        };
        let asset_key = serde_json::to_string(&tx.asset).unwrap_or_else(|_| "null".to_string());

        out.push(CandidateTransaction {
            id: tx.id,
            account_id: tx.account_id,
            account_name: tx.account_name,
            date,
            description: tx.description,
            display_name,
            normalized_name,
            compact_name,
            tokens,
            amount_abs: amount.abs(),
            amount,
            amount_raw: tx.amount,
            asset: tx.asset,
            asset_key,
            direction,
        });
    }
    Ok(out)
}

fn evaluate_recurring_candidates(
    transactions: &[CandidateTransaction],
) -> Vec<CandidateEvaluation> {
    let merchant_groups = cluster_by_similar_name(transactions);
    let mut evaluations = Vec::new();

    for group in merchant_groups {
        let group_transactions: Vec<&CandidateTransaction> =
            group.iter().map(|idx| &transactions[*idx]).collect();
        let mut candidate_groups = vec![group_transactions.clone()];
        candidate_groups.extend(amount_bucket_groups(&group_transactions));

        for candidate_group in candidate_groups {
            if let Some(evaluation) = evaluate_group(&candidate_group) {
                evaluations.push(evaluation);
            }
        }
    }

    evaluations
}

fn cluster_by_similar_name(transactions: &[CandidateTransaction]) -> Vec<Vec<usize>> {
    let mut name_entries: Vec<(String, String, String, &'static str, HashSet<String>)> = Vec::new();
    let mut name_to_index = HashMap::new();

    for tx in transactions {
        let key = (
            tx.asset_key.clone(),
            tx.direction,
            tx.normalized_name.clone(),
            tx.compact_name.clone(),
        );
        if name_to_index.contains_key(&key) {
            continue;
        }
        let idx = name_entries.len();
        name_to_index.insert(key, idx);
        name_entries.push((
            tx.normalized_name.clone(),
            tx.compact_name.clone(),
            tx.asset_key.clone(),
            tx.direction,
            tx.tokens.clone(),
        ));
    }

    let mut uf = UnionFind::new(name_entries.len());
    for i in 0..name_entries.len() {
        for j in (i + 1)..name_entries.len() {
            if name_entries[i].2 != name_entries[j].2 || name_entries[i].3 != name_entries[j].3 {
                continue;
            }
            if name_similarity(&name_entries[i], &name_entries[j]) >= 0.82 {
                uf.union(i, j);
            }
        }
    }

    let mut root_by_name: HashMap<(String, &'static str, String, String), usize> = HashMap::new();
    for (key, idx) in name_to_index {
        root_by_name.insert(key, uf.find(idx));
    }

    let mut groups_by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, tx) in transactions.iter().enumerate() {
        let key = (
            tx.asset_key.clone(),
            tx.direction,
            tx.normalized_name.clone(),
            tx.compact_name.clone(),
        );
        if let Some(root) = root_by_name.get(&key) {
            groups_by_root.entry(*root).or_default().push(idx);
        }
    }

    groups_by_root
        .into_values()
        .filter(|group| group.len() >= 2)
        .collect()
}

fn amount_bucket_groups<'a>(
    transactions: &[&'a CandidateTransaction],
) -> Vec<Vec<&'a CandidateTransaction>> {
    let mut buckets: HashMap<String, Vec<&CandidateTransaction>> = HashMap::new();
    for tx in transactions {
        let rounded = if tx.amount_abs < Decimal::new(1000, 2) {
            tx.amount_abs.round_dp(2)
        } else {
            tx.amount_abs.round_dp(0)
        };
        buckets
            .entry(rounded.normalize().to_string())
            .or_default()
            .push(*tx);
    }
    buckets
        .into_values()
        .filter(|bucket| bucket.len() >= 2 && bucket.len() < transactions.len())
        .collect()
}

fn evaluate_group(transactions: &[&CandidateTransaction]) -> Option<CandidateEvaluation> {
    if transactions.len() < 2 {
        return None;
    }

    let mut sorted = transactions.to_vec();
    sorted.sort_by_key(|tx| (tx.date, tx.id.clone()));
    sorted.dedup_by(|a, b| a.id == b.id);
    if sorted.len() < 2 {
        return None;
    }

    let dates = sorted.iter().map(|tx| tx.date).collect::<Vec<_>>();
    let cadence = best_cadence_fit(&dates)?;
    if cadence.score < 0.50 {
        return None;
    }

    let amount_score = amount_stability_score(&sorted);
    let occurrence_score = ((sorted.len() as f64 - 1.0) / 5.0).min(1.0);
    let name_score = clustered_name_score(&sorted);
    let confidence =
        0.45 * cadence.score + 0.25 * amount_score + 0.20 * occurrence_score + 0.10 * name_score;

    let status = if is_confirmed_recurring(&cadence, amount_score, confidence, sorted.len()) {
        "confirmed"
    } else {
        "possible"
    };
    let name = representative_name(&sorted);
    let normalized_name = representative_normalized_name(&sorted);
    let first_seen = sorted.first()?.date;
    let last_seen = sorted.last()?.date;
    let next_expected = next_expected_date(last_seen, &cadence).map(|date| date.to_string());
    let amount = amount_summary(&sorted);
    let reason_codes = reason_codes(&cadence, amount_score, sorted.len(), status);
    let transaction_ids = sorted
        .iter()
        .map(|tx| tx.id.clone())
        .collect::<HashSet<_>>();
    let occurrences = sorted
        .iter()
        .map(|tx| RecurringTransactionOccurrenceOutput {
            id: tx.id.clone(),
            account_id: tx.account_id.clone(),
            account_name: tx.account_name.clone(),
            date: tx.date.to_string(),
            description: tx.description.clone(),
            amount: tx.amount_raw.clone(),
        })
        .collect();

    Some(CandidateEvaluation {
        output: RecurringTransactionOutput {
            name,
            normalized_name,
            status: status.to_string(),
            cadence: cadence_label(&cadence),
            confidence: score_string(confidence),
            cadence_score: score_string(cadence.score),
            occurrence_count: sorted.len(),
            first_seen: first_seen.to_string(),
            last_seen: last_seen.to_string(),
            next_expected,
            amount,
            reason_codes,
            transactions: occurrences,
        },
        confidence,
        transaction_ids,
    })
}

fn is_confirmed_recurring(
    cadence: &CadenceFit,
    amount_score: f64,
    confidence: f64,
    occurrence_count: usize,
) -> bool {
    let enough_history_for_interval = match cadence.cadence {
        "weekly" => {
            (cadence.interval_count == 1 && occurrence_count >= 6)
                || (cadence.interval_count == 4 && occurrence_count >= 6 && amount_score >= 0.90)
        }
        "biweekly" => cadence.interval_count == 1 && occurrence_count >= 6,
        "monthly" => {
            cadence.interval_count == 1 || (cadence.interval_count <= 3 && occurrence_count >= 5)
        }
        "quarterly" | "yearly" => cadence.interval_count == 1,
        _ => false,
    };

    occurrence_count >= 3
        && confidence >= 0.80
        && cadence.score >= 0.80
        && amount_score >= 0.75
        && enough_history_for_interval
}

fn best_cadence_fit(dates: &[NaiveDate]) -> Option<CadenceFit> {
    if dates.len() < 2 {
        return None;
    }

    [
        fixed_day_cadence("weekly", dates, 7.0, 2.0),
        fixed_day_cadence("biweekly", dates, 14.0, 3.0),
        monthly_cadence("monthly", dates, 1, 5),
        monthly_cadence("quarterly", dates, 3, 10),
        monthly_cadence("yearly", dates, 12, 20),
    ]
    .into_iter()
    .flatten()
    .max_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| cadence_rank(b.cadence).cmp(&cadence_rank(a.cadence)))
    })
}

fn fixed_day_cadence(
    cadence: &'static str,
    dates: &[NaiveDate],
    base_days: f64,
    tolerance_days: f64,
) -> Option<CadenceFit> {
    let mut matched = 0usize;
    let mut multipliers = Vec::new();
    for pair in dates.windows(2) {
        let gap = (pair[1] - pair[0]).num_days();
        if gap <= 0 {
            continue;
        }
        let multiplier = ((gap as f64) / base_days).round().max(1.0);
        let expected = multiplier * base_days;
        let tolerance = tolerance_days + (multiplier - 1.0) * 1.5;
        if ((gap as f64) - expected).abs() <= tolerance {
            matched += 1;
            multipliers.push(multiplier as u32);
        }
    }

    if dates.len() < 2 {
        return None;
    }
    Some(CadenceFit {
        cadence,
        score: matched as f64 / (dates.len() - 1) as f64,
        interval_count: median_u32(&multipliers).unwrap_or(1),
    })
}

fn monthly_cadence(
    cadence: &'static str,
    dates: &[NaiveDate],
    base_months: u32,
    tolerance_days: i64,
) -> Option<CadenceFit> {
    let mut matched = 0usize;
    let mut multipliers = Vec::new();

    for pair in dates.windows(2) {
        let months = months_between(pair[0], pair[1]);
        if months < base_months {
            continue;
        }
        let multiplier = ((months as f64) / (base_months as f64)).round().max(1.0) as u32;
        let expected_months = base_months * multiplier;
        let Some(expected_date) = pair[0].checked_add_months(Months::new(expected_months)) else {
            continue;
        };
        let deviation = (pair[1] - expected_date).num_days().abs();
        if deviation <= tolerance_days + (multiplier as i64 - 1) * 2 {
            matched += 1;
            multipliers.push(multiplier);
        }
    }

    Some(CadenceFit {
        cadence,
        score: matched as f64 / (dates.len() - 1) as f64,
        interval_count: median_u32(&multipliers).unwrap_or(1),
    })
}

fn months_between(start: NaiveDate, end: NaiveDate) -> u32 {
    if end <= start {
        return 0;
    }
    let mut months =
        (end.year() - start.year()) * 12 + (end.month() as i32) - (start.month() as i32);
    if end.day() + 5 < start.day() {
        months -= 1;
    }
    months.max(0) as u32
}

fn next_expected_date(last_seen: NaiveDate, cadence: &CadenceFit) -> Option<NaiveDate> {
    match cadence.cadence {
        "weekly" => Some(last_seen + chrono::Duration::days(7 * cadence.interval_count as i64)),
        "biweekly" => Some(last_seen + chrono::Duration::days(14 * cadence.interval_count as i64)),
        "monthly" => last_seen.checked_add_months(Months::new(cadence.interval_count)),
        "quarterly" => last_seen.checked_add_months(Months::new(3 * cadence.interval_count)),
        "yearly" => last_seen.checked_add_months(Months::new(12 * cadence.interval_count)),
        _ => None,
    }
}

fn cadence_label(cadence: &CadenceFit) -> String {
    if cadence.interval_count <= 1 {
        return cadence.cadence.to_string();
    }
    match cadence.cadence {
        "weekly" => format!("every_{}_weeks", cadence.interval_count),
        "biweekly" => format!("every_{}_biweekly_periods", cadence.interval_count),
        "monthly" => format!("every_{}_months", cadence.interval_count),
        "quarterly" => format!("every_{}_quarters", cadence.interval_count),
        "yearly" => format!("every_{}_years", cadence.interval_count),
        other => other.to_string(),
    }
}

fn cadence_rank(cadence: &str) -> u8 {
    match cadence {
        "monthly" => 0,
        "weekly" => 1,
        "biweekly" => 2,
        "quarterly" => 3,
        "yearly" => 4,
        _ => 5,
    }
}

fn amount_stability_score(transactions: &[&CandidateTransaction]) -> f64 {
    let mut amounts = transactions
        .iter()
        .map(|tx| tx.amount_abs)
        .collect::<Vec<Decimal>>();
    amounts.sort();
    let min = *amounts.first().unwrap_or(&Decimal::ZERO);
    let max = *amounts.last().unwrap_or(&Decimal::ZERO);
    let median = amounts[amounts.len() / 2].max(Decimal::new(1, 0));
    let range = max - min;
    if range <= Decimal::new(1, 2) {
        1.0
    } else if range <= Decimal::new(200, 2) {
        0.95
    } else {
        let ratio = range / median;
        if ratio <= Decimal::new(5, 2) {
            0.90
        } else if ratio <= Decimal::new(15, 2) {
            0.75
        } else if ratio <= Decimal::new(50, 2) {
            0.45
        } else {
            0.20
        }
    }
}

fn clustered_name_score(transactions: &[&CandidateTransaction]) -> f64 {
    let names = transactions
        .iter()
        .map(|tx| tx.normalized_name.as_str())
        .collect::<HashSet<_>>();
    if names.len() == 1 {
        1.0
    } else if names.len() <= 3 {
        0.85
    } else {
        0.70
    }
}

fn amount_summary(transactions: &[&CandidateTransaction]) -> RecurringTransactionAmountOutput {
    let mut amounts = transactions.iter().map(|tx| tx.amount).collect::<Vec<_>>();
    amounts.sort();
    let min = *amounts.first().unwrap_or(&Decimal::ZERO);
    let max = *amounts.last().unwrap_or(&Decimal::ZERO);
    let typical = amounts[amounts.len() / 2];

    RecurringTransactionAmountOutput {
        typical: decimal_string(typical),
        min: decimal_string(min),
        max: decimal_string(max),
        asset: transactions
            .first()
            .map(|tx| tx.asset.clone())
            .unwrap_or(serde_json::Value::Null),
    }
}

fn representative_name(transactions: &[&CandidateTransaction]) -> String {
    most_common(transactions.iter().map(|tx| tx.display_name.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

fn representative_normalized_name(transactions: &[&CandidateTransaction]) -> String {
    most_common(transactions.iter().map(|tx| tx.normalized_name.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

fn most_common<'a>(values: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|(left_value, left_count), (right_value, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_value.cmp(left_value))
        })
        .map(|(value, _)| value)
}

fn reason_codes(
    cadence: &CadenceFit,
    amount_score: f64,
    occurrence_count: usize,
    status: &str,
) -> Vec<String> {
    let mut reasons = vec![
        "similar_merchant_name".to_string(),
        format!("{}_cadence", cadence.cadence),
    ];
    if cadence.score >= 0.80 {
        reasons.push("strong_cadence".to_string());
    }
    if amount_score >= 0.90 {
        reasons.push("stable_amount".to_string());
    } else if amount_score >= 0.45 {
        reasons.push("variable_but_bounded_amount".to_string());
    }
    if occurrence_count >= 4 {
        reasons.push("multiple_occurrences".to_string());
    }
    if status == "possible" {
        reasons.push("needs_more_history".to_string());
    }
    reasons
}

fn normalize_recurring_name(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let cleaned = lowered
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>();
    let raw_tokens = cleaned
        .split_whitespace()
        .map(|token| token.to_string())
        .collect::<Vec<_>>();
    let tokens = raw_tokens
        .iter()
        .filter(|token| keep_name_token(token))
        .cloned()
        .collect::<Vec<_>>();

    if !tokens.is_empty() {
        return tokens.join(" ");
    }

    if raw_tokens.len() >= 2 && raw_tokens.iter().all(|token| token.len() == 1) {
        return raw_tokens.join("");
    }

    raw_tokens.join(" ")
}

fn keep_name_token(token: &str) -> bool {
    if token.is_empty() || token.len() == 1 {
        return false;
    }
    if token.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if token.len() == 2 && US_STATE_CODES.contains(&token) {
        return false;
    }
    !NAME_STOP_WORDS.contains(&token)
}

fn name_similarity(
    left: &(String, String, String, &'static str, HashSet<String>),
    right: &(String, String, String, &'static str, HashSet<String>),
) -> f64 {
    if left.0 == right.0 {
        return 1.0;
    }
    if left.1 == right.1 {
        return 0.96;
    }
    let shorter_len = left.1.len().min(right.1.len());
    if shorter_len >= 5 && (left.1.contains(&right.1) || right.1.contains(&left.1)) {
        return 0.88;
    }
    let token_score = token_jaccard(&left.4, &right.4);
    let edit_score = edit_similarity(&left.1, &right.1);
    (token_score * 0.60).max(edit_score * 0.90)
}

fn token_jaccard(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    intersection as f64 / union as f64
}

fn edit_similarity(left: &str, right: &str) -> f64 {
    let max_len = left.chars().count().max(right.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein(left, right);
    1.0 - (distance as f64 / max_len as f64)
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right_chars = right.chars().collect::<Vec<_>>();
    let mut costs = (0..=right_chars.len()).collect::<Vec<_>>();

    for (i, left_char) in left.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = i + 1;
        for (j, right_char) in right_chars.iter().enumerate() {
            let current = costs[j + 1];
            costs[j + 1] = if left_char == *right_char {
                previous
            } else {
                1 + previous.min(current).min(costs[j])
            };
            previous = current;
        }
    }

    *costs.last().unwrap_or(&0)
}

fn median_u32(values: &[u32]) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    Some(sorted[sorted.len() / 2])
}

fn decimal_string(value: Decimal) -> String {
    value.normalize().to_string()
}

fn score_string(value: f64) -> String {
    format!("{value:.2}")
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    fn find(&mut self, idx: usize) -> usize {
        if self.parent[idx] != idx {
            self.parent[idx] = self.find(self.parent[idx]);
        }
        self.parent[idx]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root != right_root {
            self.parent[right_root] = left_root;
        }
    }
}

const NAME_STOP_WORDS: &[&str] = &[
    "ach",
    "auto",
    "autopay",
    "bill",
    "card",
    "checkcard",
    "chkcard",
    "co",
    "com",
    "debit",
    "digital",
    "electronic",
    "inc",
    "llc",
    "online",
    "payment",
    "pos",
    "purchase",
    "rec",
    "recurring",
    "sq",
    "the",
    "transaction",
    "web",
    "www",
];

const US_STATE_CODES: &[&str] = &[
    "ak", "al", "ar", "az", "ca", "co", "ct", "dc", "de", "fl", "ga", "hi", "ia", "id", "il", "in",
    "ks", "ky", "la", "ma", "md", "me", "mi", "mn", "mo", "ms", "mt", "nc", "nd", "ne", "nh", "nj",
    "nm", "nv", "ny", "oh", "ok", "or", "pa", "ri", "sc", "sd", "tn", "tx", "ut", "va", "vt", "wa",
    "wi", "wv", "wy",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, FixedClock};
    use crate::config::{
        AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
        RefreshConfig, SpendingConfig, TrayConfig,
    };
    use crate::models::{
        Account, Asset, Connection, ConnectionConfig, ConnectionState, FixedIdGenerator, Id,
        Transaction,
    };
    use crate::storage::{MemoryStorage, Storage};
    use chrono::{TimeZone, Utc};

    fn test_config() -> ResolvedConfig {
        ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: DisplayConfig::default(),
            refresh: RefreshConfig::default(),
            history: HistoryConfig::default(),
            tray: TrayConfig::default(),
            spending: SpendingConfig::default(),
            portfolio: PortfolioConfig::default(),
            ignore: IgnoreConfig::default(),
            ai: AiConfig::default(),
            git: GitConfig::default(),
        }
    }

    async fn storage_with_transactions(transactions: &[Transaction]) -> Result<MemoryStorage> {
        let storage = MemoryStorage::new();
        let conn_id = Id::from_string("conn-1");
        let account_id = Id::from_string("acct-1");
        storage
            .save_connection(&Connection {
                config: ConnectionConfig {
                    name: "Test".to_string(),
                    synchronizer: "manual".to_string(),
                    credentials: None,
                    balance_staleness: None,
                },
                state: ConnectionState::new_with(
                    conn_id.clone(),
                    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                ),
            })
            .await?;
        storage
            .save_account(&Account::new_with(
                account_id.clone(),
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                "Checking",
                conn_id,
            ))
            .await?;
        storage
            .append_transactions(&account_id, transactions)
            .await?;
        Ok(storage)
    }

    fn tx(id: &str, date: (i32, u32, u32), amount: &str, description: &str) -> Transaction {
        let ids = FixedIdGenerator::new([Id::from_string(id)]);
        let clock = FixedClock::new(
            Utc.with_ymd_and_hms(date.0, date.1, date.2, 12, 0, 0)
                .unwrap(),
        );
        Transaction::new_with_generator(&ids, &clock, amount, Asset::currency("USD"), description)
            .with_timestamp(clock.now())
    }

    #[tokio::test]
    async fn detects_monthly_recurring_transactions_with_noisy_names() -> Result<()> {
        let storage = storage_with_transactions(&[
            tx("tx-1", (2026, 1, 14), "-11.99", "SPOTIFY USA 1234"),
            tx("tx-2", (2026, 2, 14), "-11.99", "Spotify.com"),
            tx("tx-3", (2026, 3, 15), "-11.99", "SPOTIFY USA 5678"),
            tx("tx-4", (2026, 3, 20), "-42.00", "Random Store"),
        ])
        .await?;

        let out = list_recurring_transactions(
            &storage,
            RecurringTransactionsOptions {
                start: Some("2026-01-01".to_string()),
                end: Some("2026-04-01".to_string()),
                include_ignored: false,
                include_possible: false,
                min_confidence: 0.70,
            },
            &test_config(),
        )
        .await?;

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].normalized_name, "spotify usa");
        assert_eq!(out[0].cadence, "monthly");
        assert_eq!(out[0].status, "confirmed");
        assert_eq!(out[0].amount.typical, "-11.99");
        assert_eq!(out[0].occurrence_count, 3);
        Ok(())
    }

    #[tokio::test]
    async fn hides_two_occurrence_candidates_unless_possible_is_included() -> Result<()> {
        let storage = storage_with_transactions(&[
            tx("tx-1", (2026, 1, 1), "-30.00", "Gym Membership"),
            tx("tx-2", (2026, 2, 1), "-30.00", "GYM MEMBERSHIP"),
        ])
        .await?;
        let config = test_config();

        let confirmed_only = list_recurring_transactions(
            &storage,
            RecurringTransactionsOptions {
                start: Some("2026-01-01".to_string()),
                end: Some("2026-03-01".to_string()),
                include_ignored: false,
                include_possible: false,
                min_confidence: 0.50,
            },
            &config,
        )
        .await?;
        assert!(confirmed_only.is_empty());

        let possible = list_recurring_transactions(
            &storage,
            RecurringTransactionsOptions {
                start: Some("2026-01-01".to_string()),
                end: Some("2026-03-01".to_string()),
                include_ignored: false,
                include_possible: true,
                min_confidence: 0.50,
            },
            &config,
        )
        .await?;
        assert_eq!(possible.len(), 1);
        assert_eq!(possible[0].status, "possible");
        Ok(())
    }
}
