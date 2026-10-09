mod support;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::{JsonFileStorage, Storage};
use support::{git_available, init_repo, run_git};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let output = run_git(dir, args)?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn clone(remote: &Path, target: &Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .arg("clone")
        .arg(remote)
        .arg(target)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("git clone failed");
    }
    git(target, &["config", "user.email", "test@example.com"])?;
    git(target, &["config", "user.name", "Keepbook Test"])
}

#[tokio::test]
async fn sync_prices_pulls_remote_changes_first() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let root = TempDir::new()?;
    let remote = root.path().join("remote.git");
    let seed = root.path().join("seed");
    let data_dir = root.path().join("data");
    let peer = root.path().join("peer");

    std::fs::create_dir_all(&remote)?;
    git(&remote, &["init", "--bare"])?;
    std::fs::create_dir_all(&seed)?;
    init_repo(&seed)?;
    std::fs::write(seed.join("README"), "seed\n")?;
    git(&seed, &["add", "-A"])?;
    git(&seed, &["commit", "-m", "seed"])?;
    git(
        &seed,
        &["push", remote.to_str().unwrap(), "HEAD:refs/heads/master"],
    )?;
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/master"])?;

    clone(&remote, &data_dir)?;
    clone(&remote, &peer)?;
    std::fs::write(peer.join("from-peer.txt"), "peer\n")?;
    git(&peer, &["add", "-A"])?;
    git(&peer, &["commit", "-m", "peer change"])?;
    git(&peer, &["push"])?;

    let config = ResolvedConfig::load_or_default(&data_dir.join("keepbook.toml"))?;
    assert!(!config.git.pull_before_edit);
    let storage: Arc<dyn Storage> = Arc::new(JsonFileStorage::new(&data_dir));
    app::sync_prices(storage, &config, app::SyncPricesScopeArg::All, false, None).await?;

    assert!(data_dir.join("from-peer.txt").exists());
    Ok(())
}
