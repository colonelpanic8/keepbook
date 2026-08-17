use std::path::Path;
use std::process::Command;

use anyhow::Result;
use git2::{IndexAddOption, Repository, RepositoryInitOptions, Signature};
use tempfile::TempDir;

fn create_remote(path: &Path) -> Result<()> {
    let mut options = RepositoryInitOptions::new();
    options.initial_head("main");
    let repository = Repository::init_opts(path, &options)?;
    std::fs::write(path.join("keepbook.toml"), "reporting_currency = \"USD\"\n")?;
    let mut index = repository.index()?;
    index.add_all(["."], IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repository.find_tree(tree_id)?;
    let signature = Signature::now("Keepbook Tests", "keepbook@example.invalid")?;
    repository.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])?;
    Ok(())
}

#[test]
fn repositories_setup_emits_results_and_exits_nonzero_after_partial_failure() -> Result<()> {
    let temp = TempDir::new()?;
    let remote = temp.path().join("remote");
    create_remote(&remote)?;
    let ready = temp.path().join("ready");
    let occupied = temp.path().join("occupied");
    std::fs::create_dir_all(&occupied)?;
    let manifest = temp.path().join("app.toml");
    std::fs::write(
        &manifest,
        format!(
            r#"
[[repositories]]
id = "ready"
remote = "{}"
branch = "main"
path = "{}"

[[repositories]]
id = "occupied"
remote = "{}"
branch = "main"
path = "{}"
"#,
            remote.display(),
            ready.display(),
            remote.display(),
            occupied.display()
        ),
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_keepbook"))
        .args([
            "repositories",
            "setup",
            "--app-config",
            manifest.to_str().unwrap(),
        ])
        .output()?;
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["ok"], false);
    assert_eq!(json["repositories"][0]["status"], "cloned");
    assert!(json["repositories"][0].get("error").is_none());
    assert_eq!(json["repositories"][1]["status"], "error");
    assert!(json["repositories"][1]["error"].is_string());
    Ok(())
}
