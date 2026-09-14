#[derive(Debug, Default)]
pub(crate) struct SyncCounts {
    pub(crate) total: usize,
    pub(crate) synced: usize,
    pub(crate) skipped_manual: usize,
    pub(crate) skipped_not_stale: usize,
    pub(crate) failed: usize,
}

pub(crate) fn parse_sync_counts(value: &serde_json::Value) -> SyncCounts {
    let mut counts = SyncCounts {
        total: value
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize,
        ..Default::default()
    };

    let Some(results) = value.get("results").and_then(serde_json::Value::as_array) else {
        return counts;
    };

    for result in results {
        let success = result
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if !success {
            counts.failed += 1;
            continue;
        }

        let skipped = result
            .get("skipped")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if !skipped {
            counts.synced += 1;
            continue;
        }

        match result
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
        {
            "manual" => counts.skipped_manual += 1,
            "not stale" => counts.skipped_not_stale += 1,
            _ => counts.skipped_not_stale += 1,
        }
    }

    counts
}

pub(crate) fn parse_price_counts(value: &serde_json::Value) -> (usize, usize, usize) {
    let result = value.get("result").cloned().unwrap_or_default();
    let fetched = result
        .get("fetched")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let skipped = result
        .get("skipped")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let failed = result
        .get("failed_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    (fetched, skipped, failed)
}

pub(crate) fn parse_symlink_counts(value: &serde_json::Value) -> (usize, usize) {
    let connection_symlinks = value
        .get("connection_symlinks_created")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let account_symlinks = value
        .get("account_symlinks_created")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    (connection_symlinks, account_symlinks)
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/keepbook_sync_daemon/parse_tests.rs"]
mod parse_tests;
