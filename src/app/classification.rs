use serde_json::Value;

use crate::models::{TransactionAnnotation, TransactionStandardizedMetadata};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VirtualTagHierarchy {
    pub tags: Vec<String>,
    pub subtags: Vec<String>,
}

pub(crate) fn effective_transaction_tags(
    annotation: Option<&TransactionAnnotation>,
    provider_hierarchy: &VirtualTagHierarchy,
) -> Vec<String> {
    if let Some(tags) = annotation.and_then(|annotation| annotation.tags.clone()) {
        return normalize_user_labels(tags);
    }

    provider_hierarchy.tags.clone()
}

pub(crate) fn effective_transaction_subtags(
    annotation: Option<&TransactionAnnotation>,
    provider_hierarchy: &VirtualTagHierarchy,
) -> Vec<String> {
    if let Some(subtags) = annotation.and_then(|annotation| annotation.subtags.clone()) {
        return normalize_user_labels(subtags);
    }

    if annotation
        .and_then(|annotation| annotation.tags.as_ref())
        .is_some()
    {
        return Vec::new();
    }

    provider_hierarchy.subtags.clone()
}

pub(crate) fn provider_virtual_tag_hierarchy(
    metadata: Option<&TransactionStandardizedMetadata>,
    synchronizer_data: &Value,
) -> VirtualTagHierarchy {
    let mut labels = Vec::new();

    if let Some(category_path) = synchronizer_data
        .get("category")
        .and_then(|value| value.as_array())
    {
        for value in category_path {
            if let Some(label) = value.as_str().and_then(normalize_provider_label) {
                push_unique(&mut labels, label);
            }
        }
    }

    if let Some(label) = synchronizer_data
        .get("etu_standard_expense_category_code")
        .and_then(|value| value.as_str())
        .and_then(normalize_provider_label)
    {
        push_unique(&mut labels, label);
    }

    if let Some(label) = metadata
        .and_then(|metadata| metadata.merchant_category_label.as_deref())
        .and_then(normalize_provider_label)
    {
        push_unique(&mut labels, label);
    }

    if labels.is_empty() {
        return VirtualTagHierarchy::default();
    }

    let top_tag = labels
        .iter()
        .find_map(|label| known_top_level_tag(label))
        .unwrap_or_else(|| labels[0].clone());

    let mut hierarchy = VirtualTagHierarchy {
        tags: vec![top_tag.clone()],
        subtags: Vec::new(),
    };

    for label in labels {
        if !label.eq_ignore_ascii_case(&top_tag) {
            push_unique(&mut hierarchy.subtags, label);
        }
    }

    hierarchy
}

fn normalize_user_labels(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .fold(Vec::new(), |mut acc, value| {
            push_unique(&mut acc, value);
            acc
        })
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if value.trim().is_empty() {
        return;
    }
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        values.push(value);
    }
}

fn normalize_provider_label(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace(['_', '-'], " ");
    if normalized.is_empty() {
        return None;
    }

    let lower = normalized.to_lowercase();
    if matches!(
        lower.as_str(),
        "food and drink" | "food & drink" | "food drink"
    ) {
        return Some("Food & Drink".to_string());
    }

    let words = normalized
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn known_top_level_tag(label: &str) -> Option<String> {
    let normalized = label.trim().to_lowercase().replace('&', "and");
    let top = match normalized.as_str() {
        "food" | "food and drink" | "groceries" | "grocery" | "restaurants" | "restaurant"
        | "dining" | "fast food" | "coffee" | "cafes" | "cafe" | "bakeries" | "bakery"
        | "liquor" | "alcohol" => "Food",
        "home" | "housing" | "rent" | "mortgage" | "home improvement" => "Home",
        _ => return None,
    };
    Some(top.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TransactionStandardizedMetadata;

    #[test]
    fn plaid_food_hierarchy_becomes_food_tag_and_grocery_subtags() {
        let hierarchy = provider_virtual_tag_hierarchy(
            None,
            &serde_json::json!({
                "category": ["Food and Drink", "Groceries"]
            }),
        );

        assert_eq!(hierarchy.tags, vec!["Food".to_string()]);
        assert_eq!(
            hierarchy.subtags,
            vec!["Food & Drink".to_string(), "Groceries".to_string()]
        );
    }

    #[test]
    fn chase_food_category_uses_broad_code_and_merchant_category_as_subtags() {
        let metadata = TransactionStandardizedMetadata {
            merchant_name: None,
            merchant_category_code: Some("5814".to_string()),
            merchant_category_label: Some("Fast Food".to_string()),
            transaction_kind: None,
            is_internal_transfer_hint: None,
        };
        let hierarchy = provider_virtual_tag_hierarchy(
            Some(&metadata),
            &serde_json::json!({
                "etu_standard_expense_category_code": "FOOD_AND_DRINK",
                "merchant_category_name": "Fast Food"
            }),
        );

        assert_eq!(hierarchy.tags, vec!["Food".to_string()]);
        assert_eq!(
            hierarchy.subtags,
            vec!["Food & Drink".to_string(), "Fast Food".to_string()]
        );
    }

    #[test]
    fn housing_provider_labels_roll_up_to_home() {
        let hierarchy = provider_virtual_tag_hierarchy(
            None,
            &serde_json::json!({
                "category": ["Housing", "Rent"]
            }),
        );

        assert_eq!(hierarchy.tags, vec!["Home".to_string()]);
        assert_eq!(
            hierarchy.subtags,
            vec!["Housing".to_string(), "Rent".to_string()]
        );
    }

    #[test]
    fn explicit_tags_suppress_provider_virtual_subtags() {
        let annotation = TransactionAnnotation {
            transaction_id: crate::models::Id::from_string("tx-1"),
            description: None,
            note: None,
            tags: Some(vec!["Personal".to_string()]),
            subtags: None,
            effective_date: None,
        };
        let provider = VirtualTagHierarchy {
            tags: vec!["Food".to_string()],
            subtags: vec!["Groceries".to_string()],
        };

        assert_eq!(
            effective_transaction_tags(Some(&annotation), &provider),
            vec!["Personal".to_string()]
        );
        assert!(effective_transaction_subtags(Some(&annotation), &provider).is_empty());
    }
}
