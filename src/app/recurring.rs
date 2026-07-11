use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{Months, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::models::{
    Id, RecurringTransactionReview, RecurringTransactionReviewOccurrence,
    RecurringTransactionReviewStatus,
};
use crate::storage::Storage;

use super::list::list_transactions;
use super::{
    RecurringTransactionAmountOutput, RecurringTransactionOccurrenceOutput,
    RecurringTransactionOutput, RecurringTransactionReviewOccurrenceOutput,
    RecurringTransactionReviewOutput, RecurringTransactionsOptions, TransactionOutput,
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
    estimated_interval_days: Decimal,
    occurrences_per_year: Decimal,
    grace_days: i64,
    confirmed_occurrences: usize,
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
        None,
        false,
        !options.include_ignored,
        config,
    )
    .await?;

    let as_of = options
        .end
        .as_deref()
        .and_then(parse_ymd_prefix)
        .unwrap_or_else(|| Utc::now().date_naive());
    let candidates = transaction_candidates(transactions)?;
    let mut evaluations = evaluate_recurring_candidates(&candidates, as_of);
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

pub async fn list_recurring_transaction_reviews(
    storage: &dyn Storage,
) -> Result<Vec<RecurringTransactionReviewOutput>> {
    let reviews = storage.get_recurring_transaction_reviews().await?;
    Ok(reviews.into_iter().map(recurring_review_output).collect())
}

pub async fn set_recurring_transaction_review(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    candidate_key: String,
    status: RecurringTransactionReviewStatus,
    candidate: &RecurringTransactionOutput,
) -> Result<serde_json::Value> {
    let expected_key = recurring_transaction_candidate_key(candidate);
    if candidate_key != expected_key {
        anyhow::bail!("Recurring transaction candidate key does not match candidate details");
    }

    let occurrences = candidate
        .transactions
        .iter()
        .map(|occurrence| {
            let account_id = Id::from_string_checked(&occurrence.account_id)
                .with_context(|| format!("Invalid account id: {}", occurrence.account_id))?;
            let transaction_id = Id::from_string_checked(&occurrence.id)
                .with_context(|| format!("Invalid transaction id: {}", occurrence.id))?;
            Ok(RecurringTransactionReviewOccurrence {
                account_id,
                transaction_id,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let review = RecurringTransactionReview {
        candidate_key: candidate_key.clone(),
        updated_at: chrono::Utc::now(),
        status,
        name: candidate.name.clone(),
        normalized_name: candidate.normalized_name.clone(),
        cadence: candidate.cadence.clone(),
        amount_typical: candidate.amount.typical.clone(),
        asset: candidate.amount.asset.clone(),
        occurrences,
    };
    storage
        .append_recurring_transaction_reviews(std::slice::from_ref(&review))
        .await?;
    super::maybe_auto_commit(
        config,
        &format!("review recurring transaction {}", review.normalized_name),
    );

    serde_json::to_value(recurring_review_output(review)).context("serialize recurring review")
}

pub fn recurring_transaction_candidate_key(candidate: &RecurringTransactionOutput) -> String {
    let asset = serde_json::to_string(&candidate.amount.asset).unwrap_or_else(|_| "null".into());
    format!(
        "v1|{}|{}|{}|{}",
        candidate.normalized_name, candidate.cadence, candidate.amount.typical, asset
    )
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
        // Recurring costs are outflows. Deposits, refunds, and transfers may be
        // periodic, but they are not predictable recurring costs.
        if !amount.is_sign_negative() || amount.is_zero() {
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
    as_of: NaiveDate,
) -> Vec<CandidateEvaluation> {
    let merchant_groups = cluster_by_similar_name(transactions);
    let mut evaluations = Vec::new();

    for group in merchant_groups {
        let group_transactions: Vec<&CandidateTransaction> =
            group.iter().map(|idx| &transactions[*idx]).collect();
        let mut candidate_groups = vec![group_transactions.clone()];
        candidate_groups.extend(amount_bucket_groups(&group_transactions));

        for candidate_group in candidate_groups {
            if let Some(evaluation) = evaluate_group(&candidate_group, as_of) {
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
        // Exact-cent buckets separate multiple subscriptions billed by a
        // single payment processor without grouping unrelated purchases that
        // merely round to the same whole-dollar amount.
        let rounded = tx.amount_abs.round_dp(2);
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

fn evaluate_group(
    transactions: &[&CandidateTransaction],
    as_of: NaiveDate,
) -> Option<CandidateEvaluation> {
    if transactions.len() < 3 {
        return None;
    }

    let mut sorted = transactions.to_vec();
    sorted.sort_by_key(|tx| (tx.date, tx.id.clone()));
    sorted.dedup_by(|a, b| a.id == b.id);
    if sorted.len() < 3 {
        return None;
    }

    let dates = sorted.iter().map(|tx| tx.date).collect::<Vec<_>>();
    let preliminary_cadence = best_cadence_fit(&dates)?;
    let run_start = recent_schedule_run_start(&dates, &preliminary_cadence);
    if run_start > 0 {
        sorted = sorted[run_start..].to_vec();
    }
    if sorted.len() < 3 {
        return None;
    }

    let dates = sorted.iter().map(|tx| tx.date).collect::<Vec<_>>();
    let cadence = best_cadence_fit(&dates)?;
    if cadence.score < 0.75 {
        return None;
    }

    let amount_score = amount_stability_score(&sorted);
    if amount_score < 0.82 {
        return None;
    }
    let last_seen = sorted.last()?.date;
    let next_expected = next_expected_date(last_seen, &cadence)?;
    if as_of > next_expected + chrono::Duration::days(cadence.grace_days) {
        return None;
    }

    let occurrence_score = ((sorted.len() as f64 - 1.0) / 5.0).min(1.0);
    let name_score = clustered_name_score(&sorted);
    let confidence =
        0.50 * cadence.score + 0.30 * amount_score + 0.15 * occurrence_score + 0.05 * name_score;

    let status = if is_confirmed_recurring(&cadence, amount_score, confidence, sorted.len()) {
        "confirmed"
    } else {
        "possible"
    };
    let name = representative_name(&sorted);
    let normalized_name = representative_normalized_name(&sorted);
    let first_seen = sorted.first()?.date;
    let amount = amount_summary(&sorted);
    let estimated_recurring_cost = estimated_recurring_cost(&sorted);
    let estimated_annual_cost =
        (estimated_recurring_cost * cadence.occurrences_per_year).round_dp(2);
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
            cadence: cadence.cadence.to_string(),
            estimated_interval_days: decimal_string(cadence.estimated_interval_days),
            estimated_recurring_cost: decimal_string(estimated_recurring_cost),
            estimated_annual_cost: decimal_string(estimated_annual_cost),
            confidence: score_string(confidence),
            cadence_score: score_string(cadence.score),
            occurrence_count: sorted.len(),
            first_seen: first_seen.to_string(),
            last_seen: last_seen.to_string(),
            next_expected: Some(next_expected.to_string()),
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
    occurrence_count >= cadence.confirmed_occurrences
        && confidence >= 0.88
        && cadence.score >= 0.85
        && amount_score >= 0.82
}

fn best_cadence_fit(dates: &[NaiveDate]) -> Option<CadenceFit> {
    if dates.len() < 2 {
        return None;
    }

    [
        fixed_day_cadence("weekly", dates, 7, 2, 8),
        fixed_day_cadence("biweekly", dates, 14, 3, 6),
        fixed_day_cadence("every_4_weeks", dates, 28, 4, 5),
        monthly_cadence("monthly", dates, 1, 5, 4),
        monthly_cadence("every_2_months", dates, 2, 7, 4),
        monthly_cadence("quarterly", dates, 3, 10, 3),
        monthly_cadence("semiannual", dates, 6, 15, 3),
        monthly_cadence("yearly", dates, 12, 20, 3),
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
    interval_days: i64,
    tolerance_days: i64,
    confirmed_occurrences: usize,
) -> Option<CadenceFit> {
    let mut quality = 0.0;
    for pair in dates.windows(2) {
        let gap = (pair[1] - pair[0]).num_days();
        if gap <= 0 {
            continue;
        }
        let deviation = (gap - interval_days).abs();
        if deviation <= tolerance_days {
            quality += 1.0 - (deviation as f64 / (tolerance_days as f64 * 4.0 + 1.0));
        }
    }

    let interval = Decimal::from(interval_days);
    Some(CadenceFit {
        cadence,
        score: quality / (dates.len() - 1) as f64,
        estimated_interval_days: interval,
        occurrences_per_year: Decimal::new(36525, 2) / interval,
        grace_days: (interval_days / 4).max(7),
        confirmed_occurrences,
    })
}

fn monthly_cadence(
    cadence: &'static str,
    dates: &[NaiveDate],
    base_months: u32,
    tolerance_days: i64,
    confirmed_occurrences: usize,
) -> Option<CadenceFit> {
    let mut quality = 0.0;

    for pair in dates.windows(2) {
        let Some(expected_date) = pair[0].checked_add_months(Months::new(base_months)) else {
            continue;
        };
        let deviation = (pair[1] - expected_date).num_days().abs();
        if deviation <= tolerance_days {
            quality += 1.0 - (deviation as f64 / (tolerance_days as f64 * 4.0 + 1.0));
        }
    }

    let interval_days =
        (Decimal::new(36525, 2) * Decimal::from(base_months) / Decimal::from(12u32)).round_dp(2);
    Some(CadenceFit {
        cadence,
        score: quality / (dates.len() - 1) as f64,
        estimated_interval_days: interval_days,
        occurrences_per_year: Decimal::from(12u32) / Decimal::from(base_months),
        grace_days: (tolerance_days * 2).max(10),
        confirmed_occurrences,
    })
}

fn recent_schedule_run_start(dates: &[NaiveDate], cadence: &CadenceFit) -> usize {
    dates
        .windows(2)
        .enumerate()
        .rev()
        .find_map(|(index, pair)| {
            (!cadence_gap_matches(pair[0], pair[1], cadence.cadence)).then_some(index + 1)
        })
        .unwrap_or(0)
}

fn cadence_gap_matches(start: NaiveDate, end: NaiveDate, cadence: &str) -> bool {
    let fixed = match cadence {
        "weekly" => Some((7, 2)),
        "biweekly" => Some((14, 3)),
        "every_4_weeks" => Some((28, 4)),
        _ => None,
    };
    if let Some((interval, tolerance)) = fixed {
        return ((end - start).num_days() - interval).abs() <= tolerance;
    }

    let calendar = match cadence {
        "monthly" => Some((1, 5)),
        "every_2_months" => Some((2, 7)),
        "quarterly" => Some((3, 10)),
        "semiannual" => Some((6, 15)),
        "yearly" => Some((12, 20)),
        _ => None,
    };
    let Some((months, tolerance)) = calendar else {
        return false;
    };
    start
        .checked_add_months(Months::new(months))
        .is_some_and(|expected| (end - expected).num_days().abs() <= tolerance)
}

fn next_expected_date(last_seen: NaiveDate, cadence: &CadenceFit) -> Option<NaiveDate> {
    match cadence.cadence {
        "weekly" => Some(last_seen + chrono::Duration::days(7)),
        "biweekly" => Some(last_seen + chrono::Duration::days(14)),
        "every_4_weeks" => Some(last_seen + chrono::Duration::days(28)),
        "monthly" => last_seen.checked_add_months(Months::new(1)),
        "every_2_months" => last_seen.checked_add_months(Months::new(2)),
        "quarterly" => last_seen.checked_add_months(Months::new(3)),
        "semiannual" => last_seen.checked_add_months(Months::new(6)),
        "yearly" => last_seen.checked_add_months(Months::new(12)),
        _ => None,
    }
}

fn cadence_rank(cadence: &str) -> u8 {
    match cadence {
        "monthly" => 0,
        "every_4_weeks" => 1,
        "biweekly" => 2,
        "weekly" => 3,
        "every_2_months" => 4,
        "quarterly" => 5,
        "semiannual" => 6,
        "yearly" => 7,
        _ => 8,
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
    if range <= Decimal::new(10, 2) {
        1.0
    } else {
        let ratio = range / median;
        if ratio <= Decimal::new(2, 2) {
            0.98
        } else if ratio <= Decimal::new(5, 2) {
            0.92
        } else if ratio <= Decimal::new(10, 2) {
            0.82
        } else if ratio <= Decimal::new(15, 2) {
            0.70
        } else {
            0.30
        }
    }
}

fn estimated_recurring_cost(transactions: &[&CandidateTransaction]) -> Decimal {
    let mut amounts = transactions
        .iter()
        .map(|tx| tx.amount_abs)
        .collect::<Vec<_>>();
    amounts.sort();
    amounts[amounts.len() / 2].round_dp(2)
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
    reasons.push("active_pattern".to_string());
    if status == "possible" {
        reasons.push("limited_history".to_string());
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

fn decimal_string(value: Decimal) -> String {
    value.normalize().to_string()
}

fn parse_ymd_prefix(value: &str) -> Option<NaiveDate> {
    value
        .get(..10)
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

fn score_string(value: f64) -> String {
    format!("{value:.2}")
}

fn recurring_review_output(review: RecurringTransactionReview) -> RecurringTransactionReviewOutput {
    RecurringTransactionReviewOutput {
        candidate_key: review.candidate_key,
        updated_at: review.updated_at.to_rfc3339(),
        status: recurring_review_status_string(review.status).to_string(),
        name: review.name,
        normalized_name: review.normalized_name,
        cadence: review.cadence,
        amount_typical: review.amount_typical,
        asset: review.asset,
        transactions: review
            .occurrences
            .into_iter()
            .map(|occurrence| RecurringTransactionReviewOccurrenceOutput {
                account_id: occurrence.account_id.to_string(),
                transaction_id: occurrence.transaction_id.to_string(),
            })
            .collect(),
    }
}

fn recurring_review_status_string(status: RecurringTransactionReviewStatus) -> &'static str {
    match status {
        RecurringTransactionReviewStatus::Verified => "verified",
        RecurringTransactionReviewStatus::Dismissed => "dismissed",
    }
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
#[path = "../../tests/unit/app/recurring_tests.rs"]
mod recurring_tests;
