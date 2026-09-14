use super::*;

pub(crate) fn transaction_tag_options(
    transactions: &[Transaction],
    tags: &[SpendingBreakdownEntry],
) -> Vec<String> {
    let mut options = tags
        .iter()
        .map(|entry| entry.key.clone())
        .filter(|key| !is_untagged_spending_key(key))
        .chain(transactions.iter().flat_map(transaction_tags))
        .filter(|tag| !is_ignore_spending_tag(tag))
        .collect::<Vec<_>>();
    options.sort_by(|a, b| compare_case_insensitive(a, b));
    options.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    options
}

pub(crate) fn filtered_transactions(
    transactions: &[Transaction],
    selected_tag: Option<&str>,
    selected_period: Option<(&str, &str)>,
    title_filter: &str,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
    show_ignored: bool,
) -> Vec<Transaction> {
    let normalized_title_filter = title_filter.trim().to_lowercase();
    let mut filtered = transactions
        .iter()
        .filter(|transaction| {
            if transaction.ignored_from_spending && !show_ignored {
                return false;
            }
            if let Some((start_date, end_date)) = selected_period {
                let date = transaction_date(transaction);
                if date.as_str() < start_date || date.as_str() > end_date {
                    return false;
                }
            }
            if !normalized_title_filter.is_empty()
                && !transaction_description(transaction)
                    .to_lowercase()
                    .contains(&normalized_title_filter)
            {
                return false;
            }
            selected_tag
                .map(|tag| transaction_has_tag(transaction, tag))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|a, b| compare_transactions(a, b, sort_field, sort_direction));
    filtered
}

pub(crate) fn compare_transactions(
    a: &Transaction,
    b: &Transaction,
    sort_field: TransactionSortField,
    sort_direction: SortDirection,
) -> std::cmp::Ordering {
    let primary = match sort_field {
        TransactionSortField::Date => a.timestamp.cmp(&b.timestamp),
        TransactionSortField::Amount => compare_transaction_amounts(a, b),
        TransactionSortField::Description => {
            compare_case_insensitive(&transaction_description(a), &transaction_description(b))
        }
        TransactionSortField::Tag => {
            compare_case_insensitive(&transaction_tags_label(a), &transaction_tags_label(b))
        }
        TransactionSortField::Account => compare_case_insensitive(&a.account_name, &b.account_name),
    };

    let primary = match sort_direction {
        SortDirection::Asc => primary,
        SortDirection::Desc => primary.reverse(),
    };

    primary
        .then_with(|| b.timestamp.cmp(&a.timestamp))
        .then_with(|| a.account_name.cmp(&b.account_name))
        .then_with(|| a.id.cmp(&b.id))
}

pub(crate) fn ai_rule_transaction_input(transaction: &Transaction) -> AiRuleTransactionInput {
    AiRuleTransactionInput {
        id: transaction.id.clone(),
        account_id: transaction.account_id.clone(),
        account_name: transaction.account_name.clone(),
        timestamp: transaction.timestamp.clone(),
        description: transaction_description(transaction),
        amount: transaction.amount.clone(),
        status: transaction.status.clone(),
        tag: transaction_tags(transaction).first().cloned(),
        subtag: transaction_subtags(transaction).first().cloned(),
        ignored_from_spending: transaction.ignored_from_spending,
    }
}

pub(crate) fn ai_tool_label(name: &str) -> &'static str {
    match name {
        "propose_tag_rule" => "Tag rule",
        "propose_ignore_rule" => "Ignore rule",
        "propose_rename_rule" => "Rename rule",
        _ => "Tool call",
    }
}

pub(crate) fn format_json_value(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(crate) fn default_transaction_sort_direction(field: TransactionSortField) -> SortDirection {
    match field {
        TransactionSortField::Date | TransactionSortField::Amount => SortDirection::Desc,
        TransactionSortField::Description
        | TransactionSortField::Tag
        | TransactionSortField::Account => SortDirection::Asc,
    }
}

pub(crate) fn sort_direction_arrow(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => "↑",
        SortDirection::Desc => "↓",
    }
}

pub(crate) fn compare_transaction_amounts(a: &Transaction, b: &Transaction) -> std::cmp::Ordering {
    let left = parse_money_input(&a.amount);
    let right = parse_money_input(&b.amount);
    match (left, right) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.amount.cmp(&b.amount),
    }
}

pub(crate) fn transaction_key(transaction: &Transaction) -> String {
    format!("{}:{}", transaction.account_id, transaction.id)
}

pub(crate) fn transaction_row_class(transaction: &Transaction) -> &'static str {
    if transaction.ignored_from_spending {
        "table-row ignored-transaction-row"
    } else {
        "table-row"
    }
}

/// Excluded from spending by a per-transaction annotation, which the row
/// editor can toggle off.
pub(crate) fn spending_ignore_is_annotation(transaction: &Transaction) -> bool {
    transaction.spending_ignore_reason.as_deref() == Some("annotation")
}

/// Excluded from spending by configuration (an ignore rule, an internal-transfer
/// hint, or an ignored account) rather than by an annotation, so the per-row
/// toggle cannot change it.
pub(crate) fn spending_ignore_is_configured(transaction: &Transaction) -> bool {
    matches!(
        transaction.spending_ignore_reason.as_deref(),
        Some("rule" | "internal_transfer" | "account")
    )
}

/// Never counted because of the transaction's own shape: only posted outflows
/// count toward spending.
pub(crate) fn spending_ignore_is_shape(transaction: &Transaction) -> bool {
    matches!(
        transaction.spending_ignore_reason.as_deref(),
        Some("not_posted" | "not_outflow")
    )
}

/// Tag spellings that mark a transaction as ignored-from-spending when present
/// on its annotation tag list.
const IGNORE_SPENDING_TAGS: [&str; 3] = ["ignore_spending", "ignore-spending", "ignore:spending"];

pub(crate) fn is_ignore_spending_tag(tag: &str) -> bool {
    IGNORE_SPENDING_TAGS
        .iter()
        .any(|candidate| tag.trim().eq_ignore_ascii_case(candidate))
}

/// Tags to display for a transaction row, hiding the legacy ignore-spending
/// control tags (the "Not counted" badge communicates that state instead).
/// Editing surfaces should keep using [`transaction_tags`] so saves round-trip
/// the full list.
pub(crate) fn visible_transaction_tags(transaction: &Transaction) -> Vec<String> {
    transaction_tags(transaction)
        .into_iter()
        .filter(|tag| !is_ignore_spending_tag(tag))
        .collect()
}

pub(crate) fn transaction_date(transaction: &Transaction) -> String {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.effective_date.clone())
        .unwrap_or_else(|| {
            transaction
                .timestamp
                .get(..10)
                .unwrap_or(&transaction.timestamp)
                .to_string()
        })
}

pub(crate) fn transaction_description(transaction: &Transaction) -> String {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.description.clone())
        .unwrap_or_else(|| transaction.description.clone())
}

pub(crate) fn transaction_subtags(transaction: &Transaction) -> Vec<String> {
    transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.subtags.clone())
        .unwrap_or_else(|| transaction.subtags.clone())
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !is_untagged_spending_key(value))
        .fold(Vec::<String>::new(), |mut acc, subtag| {
            if !acc
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&subtag))
            {
                acc.push(subtag);
            }
            acc
        })
}

pub(crate) fn transaction_tags(transaction: &Transaction) -> Vec<String> {
    if !transaction.tags.is_empty() {
        return normalize_tags(transaction.tags.clone());
    }
    if let Some(tags) = transaction
        .annotation
        .as_ref()
        .and_then(|annotation| annotation.tags.clone())
    {
        return normalize_tags(tags);
    }

    Vec::new()
}

pub(crate) fn transaction_has_tag(transaction: &Transaction, tag: &str) -> bool {
    let tags = transaction_tags(transaction);
    if is_untagged_tag(tag) && tags.is_empty() {
        return true;
    }
    tags.iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(tag))
}

pub(crate) fn is_untagged_tag(tag: &str) -> bool {
    tag.trim().eq_ignore_ascii_case("untagged")
}

pub(crate) fn transaction_tags_label(transaction: &Transaction) -> String {
    let tags = transaction_tags(transaction);
    if tags.is_empty() {
        "Untagged".to_string()
    } else {
        tags.join(", ")
    }
}

pub(crate) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
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
        })
}

pub(crate) fn format_transaction_amount(transaction: &Transaction, currency: &str) -> String {
    format_money_text(&transaction.amount, currency).unwrap_or_else(|| transaction.amount.clone())
}

pub(crate) fn proposed_patch_summary(patch: &ProposedTransactionEditPatch) -> String {
    let mut parts = Vec::new();
    push_patch_part(&mut parts, "description", &patch.description);
    push_patch_part(&mut parts, "note", &patch.note);
    if let Some(value) = &patch.tags {
        match value {
            Some(tags) => parts.push(format!("tags={}", tags.join(", "))),
            None => parts.push("tags=clear".to_string()),
        }
    }
    if let Some(value) = &patch.subtags {
        match value {
            Some(subtags) => parts.push(format!("subtags={}", subtags.join(", "))),
            None => parts.push("subtags=clear".to_string()),
        }
    }
    push_patch_part(&mut parts, "effective_date", &patch.effective_date);
    if parts.is_empty() {
        "No changes".to_string()
    } else {
        parts.join("; ")
    }
}

pub(crate) fn push_patch_part(
    parts: &mut Vec<String>,
    label: &str,
    value: &Option<Option<String>>,
) {
    if let Some(value) = value {
        match value {
            Some(text) => parts.push(format!("{label}={text}")),
            None => parts.push(format!("{label}=clear")),
        }
    }
}

pub(crate) fn proposal_action_past_tense(action: &str) -> &'static str {
    match action {
        "approve" => "Approved",
        "reject" => "Rejected",
        "remove" => "Removed",
        _ => "Updated",
    }
}
