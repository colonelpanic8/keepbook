use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::{ImportCommand, SchwabImportCommand};

pub async fn run(
    command: ImportCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        ImportCommand::Schwab(schwab_cmd) => match schwab_cmd {
            SchwabImportCommand::Transactions { account, file } => {
                let result =
                    app::import_schwab_transactions(storage_arc.as_ref(), config, &account, &file)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },
    }
    Ok(())
}
