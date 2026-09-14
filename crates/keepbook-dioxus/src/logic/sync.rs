pub(crate) fn sync_result_summary(result: &serde_json::Value) -> String {
    if let Some(results) = result.get("results").and_then(|value| value.as_array()) {
        let total = results.len();
        let synced = results
            .iter()
            .filter(|row| row.get("success").and_then(|v| v.as_bool()) == Some(true))
            .count();
        let failed = results
            .iter()
            .filter(|row| row.get("success").and_then(|v| v.as_bool()) == Some(false))
            .count();
        let skipped = results
            .iter()
            .filter(|row| row.get("skipped").and_then(|v| v.as_bool()) == Some(true))
            .count();
        return format!(
            "Balance refresh complete: {synced}/{total} ok, {skipped} skipped, {failed} failed."
        );
    }

    let connection = result
        .get("connection")
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("name").and_then(|v| v.as_str()))
        })
        .unwrap_or("connection");
    if result.get("success").and_then(|v| v.as_bool()) == Some(true) {
        if result.get("skipped").and_then(|v| v.as_bool()) == Some(true) {
            let reason = result
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("skipped");
            format!("Balance refresh skipped for {connection}: {reason}.")
        } else {
            format!("Balance refresh complete for {connection}.")
        }
    } else {
        let error = result
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown error");
        format!("Balance refresh failed for {connection}: {error}")
    }
}

pub(crate) fn price_sync_result_summary(result: &serde_json::Value) -> String {
    let Some(refresh) = result.get("result") else {
        return "Price refresh finished.".to_string();
    };
    let fetched = refresh
        .get("fetched")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let skipped = refresh
        .get("skipped")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let failed = refresh
        .get("failed_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if failed == 0 {
        format!("Price refresh complete: {fetched} fetched, {skipped} skipped.")
    } else {
        format!("Price refresh complete: {fetched} fetched, {skipped} skipped, {failed} failed.")
    }
}
