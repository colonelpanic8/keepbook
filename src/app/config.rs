use std::path::Path;

use crate::config::ResolvedConfig;

pub fn config_output(config_path: &Path, config: &ResolvedConfig) -> serde_json::Value {
    serde_json::json!({
        "config_file": config_path.display().to_string(),
        "data_directory": config.data_dir.display().to_string(),
        "git": {
            "auto_commit": config.git.auto_commit,
            "auto_push": config.git.auto_push
        }
    })
}
