use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::{AddCommand, ProposeCommand, ProposedEditsCommand, RemoveCommand, SetCommand};

pub async fn add(
    command: AddCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        AddCommand::Connection { name, synchronizer } => {
            let result =
                app::add_connection(storage_arc.as_ref(), config, &name, &synchronizer).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        AddCommand::Account {
            connection,
            name,
            tag,
        } => {
            let result =
                app::add_account(storage_arc.as_ref(), config, &connection, &name, tag).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

pub async fn remove(
    command: RemoveCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        RemoveCommand::Connection { id } => {
            let result = app::remove_connection(storage_arc.as_ref(), config, &id).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

pub async fn set(
    command: SetCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        SetCommand::Balance {
            account,
            asset,
            amount,
            cost_basis,
        } => {
            let result = app::set_balance(
                storage_arc.as_ref(),
                config,
                &account,
                &asset,
                &amount,
                cost_basis.as_deref(),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SetCommand::AccountConfig {
            account,
            balance_backfill,
            clear_balance_backfill,
        } => {
            let result = app::set_account_config(
                storage_arc.as_ref(),
                config,
                &account,
                balance_backfill.as_deref(),
                clear_balance_backfill,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SetCommand::Transaction {
            account,
            transaction,
            description,
            clear_description,
            note,
            clear_note,
            tag,
            tags_empty,
            clear_tags,
            subtag,
            subtags_empty,
            clear_subtags,
            effective_date,
            clear_effective_date,
        } => {
            let result = app::set_transaction_annotation(
                storage_arc.as_ref(),
                config,
                &account,
                &transaction,
                app::TransactionAnnotationInput {
                    description,
                    clear_description,
                    note,
                    clear_note,
                    tags: tag,
                    tags_empty,
                    clear_tags,
                    subtags: subtag,
                    subtags_empty,
                    clear_subtags,
                    effective_date,
                    clear_effective_date,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        SetCommand::TransactionIgnore {
            account,
            transaction,
            clear,
        } => {
            let targets = transaction
                .into_iter()
                .map(|transaction_id| (account.clone(), transaction_id))
                .collect::<Vec<_>>();
            let result =
                app::set_transaction_ignore(storage_arc.as_ref(), config, targets, !clear).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

pub async fn propose(
    command: ProposeCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        ProposeCommand::Transaction {
            account,
            transaction,
            description,
            clear_description,
            note,
            clear_note,
            tag,
            tags_empty,
            clear_tags,
            subtag,
            subtags_empty,
            clear_subtags,
            effective_date,
            clear_effective_date,
        } => {
            let result = app::propose_transaction_edit(
                storage_arc.as_ref(),
                config,
                &account,
                &transaction,
                app::TransactionAnnotationInput {
                    description,
                    clear_description,
                    note,
                    clear_note,
                    tags: tag,
                    tags_empty,
                    clear_tags,
                    subtags: subtag,
                    subtags_empty,
                    clear_subtags,
                    effective_date,
                    clear_effective_date,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

pub async fn proposed_edits(
    command: ProposedEditsCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        ProposedEditsCommand::List { include_decided } => {
            let result =
                app::list_proposed_transaction_edits(storage_arc.as_ref(), include_decided).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ProposedEditsCommand::Approve { id } => {
            let result =
                app::approve_proposed_transaction_edit(storage_arc.as_ref(), config, &id).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ProposedEditsCommand::Reject { id } => {
            let result =
                app::reject_proposed_transaction_edit(storage_arc.as_ref(), config, &id).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ProposedEditsCommand::Remove { id } => {
            let result =
                app::remove_proposed_transaction_edit(storage_arc.as_ref(), config, &id).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}
