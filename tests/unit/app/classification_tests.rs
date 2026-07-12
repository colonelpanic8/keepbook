use super::*;
use crate::models::TransactionStandardizedMetadata;
use std::collections::HashMap;

fn tags_config() -> TagsConfig {
    TagsConfig {
        aliases: HashMap::from([("Food And Drink".to_string(), "Food".to_string())]),
        parents: HashMap::from([
            ("Groceries".to_string(), vec!["Food".to_string()]),
            ("Fast Food".to_string(), vec!["Food".to_string()]),
            ("Rent".to_string(), vec!["Housing".to_string()]),
            ("Housing".to_string(), vec!["Home".to_string()]),
        ]),
    }
}

#[test]
fn configured_hierarchy_maps_provider_label_to_top_level_tag() {
    let config = tags_config();
    let hierarchy = provider_virtual_tag_hierarchy(
        None,
        &serde_json::json!({
            "category": ["Food and Drink", "Groceries"]
        }),
        &config,
    );

    assert_eq!(hierarchy.tags, vec!["Food".to_string()]);
    assert_eq!(hierarchy.subtags, vec!["Groceries".to_string()]);
}

#[test]
fn provider_label_without_config_becomes_simple_tag() {
    let hierarchy = provider_virtual_tag_hierarchy(
        None,
        &serde_json::json!({
            "category": ["Groceries"]
        }),
        &TagsConfig::default(),
    );

    assert_eq!(hierarchy.tags, vec!["Groceries".to_string()]);
    assert!(hierarchy.subtags.is_empty());
}

#[test]
fn chase_provider_fields_use_configured_aliases_and_parents() {
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
        &tags_config(),
    );

    assert_eq!(hierarchy.tags, vec!["Food".to_string()]);
    assert_eq!(hierarchy.subtags, vec!["Fast Food".to_string()]);
}

#[test]
fn recursive_hierarchy_finds_top_level_parent() {
    let hierarchy = provider_virtual_tag_hierarchy(
        None,
        &serde_json::json!({
            "category": ["Housing", "Rent"]
        }),
        &tags_config(),
    );

    assert_eq!(hierarchy.tags, vec!["Home".to_string()]);
    assert_eq!(
        hierarchy.subtags,
        vec!["Housing".to_string(), "Rent".to_string()]
    );
}

#[test]
fn explicit_child_tag_implies_configured_parent_tag() {
    let annotation = TransactionAnnotation {
        transaction_id: crate::models::Id::from_string("tx-1"),
        description: None,
        note: None,
        tags: Some(vec!["Groceries".to_string()]),
        subtags: None,
        effective_date: None,
        ignore_spending: None,
    };
    let provider = VirtualTagHierarchy::default();
    let config = tags_config();

    assert_eq!(
        effective_transaction_tags(Some(&annotation), &provider, &config),
        vec!["Food".to_string()]
    );
    assert_eq!(
        effective_transaction_subtags(Some(&annotation), &provider, &config),
        vec!["Groceries".to_string()]
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
        ignore_spending: None,
    };
    let provider = VirtualTagHierarchy {
        tags: vec!["Food".to_string()],
        subtags: vec!["Groceries".to_string()],
    };

    assert_eq!(
        effective_transaction_tags(Some(&annotation), &provider, &TagsConfig::default()),
        vec!["Personal".to_string()]
    );
    assert!(
        effective_transaction_subtags(Some(&annotation), &provider, &TagsConfig::default())
            .is_empty()
    );
}
