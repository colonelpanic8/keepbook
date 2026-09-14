use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::{AuthCommand, ChaseAuthCommand, SchwabAuthCommand};

pub async fn run(
    command: AuthCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        AuthCommand::Schwab(schwab_cmd) => match schwab_cmd {
            SchwabAuthCommand::Login { id_or_name } => {
                let result =
                    app::schwab_login(storage_arc.clone(), config, id_or_name.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },
        AuthCommand::Chase(chase_cmd) => match chase_cmd {
            ChaseAuthCommand::Login { id_or_name } => {
                let result =
                    app::chase_login(storage_arc.clone(), config, id_or_name.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },
    }
    Ok(())
}
