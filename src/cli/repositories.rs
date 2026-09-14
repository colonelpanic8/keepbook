use std::path::PathBuf;

use clap::Subcommand;
use keepbook::repositories::default_app_config_path;

#[derive(Subcommand)]
pub enum RepositoriesCommand {
    /// Clone missing manifest repositories and validate existing checkouts
    Setup {
        /// Path to the read-only repository manifest
        #[arg(long, default_value_os_t = default_app_config_path())]
        app_config: PathBuf,
    },
}
