use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::TransactionRulesCommand;

pub async fn run(
    command: TransactionRulesCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        TransactionRulesCommand::Add {
            set_tags,
            set_subtags,
            set_description,
            match_account_id,
            match_account_name,
            match_description,
            match_tag,
            match_subtag,
            match_status,
            match_amount,
        } => {
            let result = app::add_transaction_rule(
                config,
                app::TransactionRule {
                    set_tags: if set_tags.is_empty() {
                        None
                    } else {
                        Some(set_tags)
                    },
                    set_subtags: if set_subtags.is_empty() {
                        None
                    } else {
                        Some(set_subtags)
                    },
                    set_description,
                    match_account_id,
                    match_account_name,
                    match_description,
                    match_tag,
                    match_subtag,
                    match_status,
                    match_amount,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        TransactionRulesCommand::List => {
            let result = app::list_transaction_rules(config)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        TransactionRulesCommand::Apply {
            start,
            end,
            account,
            connection,
            overwrite,
            dry_run,
        } => {
            let result = app::apply_transaction_rules(
                storage_arc.as_ref(),
                config,
                app::ApplyTransactionRulesOptions {
                    start,
                    end,
                    account,
                    connection,
                    overwrite,
                    dry_run,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}
