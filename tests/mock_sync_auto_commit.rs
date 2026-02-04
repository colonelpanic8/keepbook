mod support;

use anyhow::Result;
use keepbook::git::{try_auto_commit, AutoCommitOutcome};
use keepbook::storage::JsonFileStorage;
use support::{git_available, init_repo, mock_connection, run_git, MockSynchronizer};
use tempfile::TempDir;

#[tokio::test]
async fn test_mock_sync_auto_commit() -> Result<()> {
    if !git_available() {
        return Ok(());
    }

    let dir = TempDir::new()?;
    init_repo(dir.path())?;

    let storage = JsonFileStorage::new(dir.path());
    let mut connection = mock_connection("Mock Bank");
    let synchronizer = MockSynchronizer::new();
    let result = synchronizer.sync(&mut connection).await?;
    result.save(&storage).await?;

    let outcome = try_auto_commit(dir.path(), "sync mock")?;
    assert_eq!(outcome, AutoCommitOutcome::Committed);

    let log = run_git(dir.path(), &["log", "-1", "--pretty=%s"])?;
    let subject = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(subject, "keepbook: sync mock");

    let status = run_git(dir.path(), &["status", "--porcelain"])?;
    let status_output = String::from_utf8_lossy(&status.stdout);
    assert!(status_output.trim().is_empty());

    // Optional sanity check that we actually wrote data
    let accounts_dir = dir.path().join("accounts");
    assert!(accounts_dir.exists());
    let connections_dir = dir.path().join("connections");
    assert!(connections_dir.exists());

    Ok(())
}
