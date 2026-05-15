use serde_json::Value;

use crate::config::TagsConfig;
use crate::models::{TransactionAnnotation, TransactionStandardizedMetadata};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VirtualTagHierarchy {
    pub tags: Vec<String>,
    pub subtags: Vec<String>,
}

pub(crate) fn effective_transaction_tags(
    annotation: Option<&TransactionAnnotation>,
    provider_hierarchy: &VirtualTagHierarchy,
    tags_config: &TagsConfig,
) -> Vec<String> {
    if let Some(hierarchy) = explicit_annotation_hierarchy(annotation, tags_config) {
        return hierarchy.tags;
    }

    provider_hierarchy.tags.clone()
}

pub(crate) fn effective_transaction_subtags(
    annotation: Option<&TransactionAnnotation>,
    provider_hierarchy: &VirtualTagHierarchy,
    tags_config: &TagsConfig,
) -> Vec<String> {
    if let Some(hierarchy) = explicit_annotation_hierarchy(annotation, tags_config) {
        return hierarchy.subtags;
    }

    provider_hierarchy.subtags.clone()
}

pub(crate) fn provider_virtual_tag_hierarchy(
    metadata: Option<&TransactionStandardizedMetadata>,
    synchronizer_data: &Value,
    tags_config: &TagsConfig,
) -> VirtualTagHierarchy {
    let mut labels = Vec::new();

    if let Some(category_path) = synchronizer_data
        .get("category")
        .and_then(|value| value.as_array())
    {
        for value in category_path {
            if let Some(label) = value
                .as_str()
                .and_then(|value| tags_config.canonical_provider_label(value))
            {
                push_unique(&mut labels, label);
            }
        }
    }

    if let Some(label) = synchronizer_data
        .get("etu_standard_expense_category_code")
        .and_then(|value| value.as_str())
        .and_then(|value| tags_config.canonical_provider_label(value))
    {
        push_unique(&mut labels, label);
    }

    if let Some(label) = metadata
        .and_then(|metadata| metadata.merchant_category_label.as_deref())
        .and_then(|value| tags_config.canonical_provider_label(value))
    {
        push_unique(&mut labels, label);
    }

    hierarchy_for_labels(labels, tags_config)
}

fn explicit_annotation_hierarchy(
    annotation: Option<&TransactionAnnotation>,
    tags_config: &TagsConfig,
) -> Option<VirtualTagHierarchy> {
    let annotation = annotation?;
    if annotation.tags.is_none() && annotation.subtags.is_none() {
        return None;
    }

    let mut hierarchy = VirtualTagHierarchy::default();
    if let Some(tags) = &annotation.tags {
        for tag in tags {
            let Some(label) = tags_config.canonical_label(tag) else {
                continue;
            };
            let roots = tags_config.root_tags_for(&label);
            for root in &roots {
                push_unique(&mut hierarchy.tags, root.clone());
            }
            if !roots.iter().any(|root| root.eq_ignore_ascii_case(&label)) {
                push_unique(&mut hierarchy.subtags, label);
            }
        }
    }

    if let Some(subtags) = &annotation.subtags {
        for subtag in subtags {
            let Some(label) = tags_config.canonical_label(subtag) else {
                continue;
            };
            for root in tags_config.parent_root_tags_for(&label) {
                push_unique(&mut hierarchy.tags, root);
            }
            if !hierarchy
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(&label))
            {
                push_unique(&mut hierarchy.subtags, label);
            }
        }
    }

    Some(hierarchy)
}

fn hierarchy_for_labels(labels: Vec<String>, tags_config: &TagsConfig) -> VirtualTagHierarchy {
    let labels = tags_config.canonicalize_labels(labels);
    let mut hierarchy = VirtualTagHierarchy::default();

    for label in &labels {
        let roots = tags_config.root_tags_for(label);
        for root in roots {
            push_unique(&mut hierarchy.tags, root);
        }
    }

    for label in labels {
        if !hierarchy
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(&label))
        {
            push_unique(&mut hierarchy.subtags, label);
        }
    }

    hierarchy
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

#[cfg(test)]
#[path = "../../tests/unit/app/classification_tests.rs"]
mod classification_tests;
