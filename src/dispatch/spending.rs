use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::{SpendingArgs, SpendingTagsArgs};

pub async fn spending(
    args: SpendingArgs,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    let SpendingArgs {
        period,
        period_alignment,
        start,
        end,
        currency,
        tz,
        week_start,
        bucket,
        account,
        connection,
        status,
        direction,
        group_by,
        top,
        lookback_days,
        include_noncurrency,
        include_empty,
    } = args;
    let output = app::spending_report(
        storage_arc.as_ref(),
        config,
        app::SpendingReportOptions {
            currency,
            start,
            end,
            period,
            period_alignment: Some(period_alignment),
            tz,
            week_start,
            bucket,
            account,
            connection,
            status,
            direction,
            group_by,
            top,
            lookback_days,
            include_noncurrency,
            include_empty,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub async fn spending_tags(
    args: SpendingTagsArgs,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    let SpendingTagsArgs {
        period,
        period_alignment,
        start,
        end,
        currency,
        tz,
        week_start,
        bucket,
        account,
        connection,
        status,
        direction,
        top,
        lookback_days,
        include_noncurrency,
        include_empty,
    } = args;
    let output = app::spending_report(
        storage_arc.as_ref(),
        config,
        app::SpendingReportOptions {
            currency,
            start,
            end,
            period,
            period_alignment: Some(period_alignment),
            tz,
            week_start,
            bucket,
            account,
            connection,
            status,
            direction,
            group_by: "tag".to_string(),
            top,
            lookback_days,
            include_noncurrency,
            include_empty,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
