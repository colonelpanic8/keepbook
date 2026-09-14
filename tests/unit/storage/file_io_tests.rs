use super::*;

#[tokio::test]
async fn failed_update_leaves_existing_jsonl_unchanged() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log.jsonl");
    let original = b"{\"value\":1}\n";
    fs::write(&path, original)?;
    let result = update_jsonl::<serde_json::Value, ()>(&path, |items| {
        items.clear();
        anyhow::bail!("injected transform failure")
    })
    .await;
    assert!(result.is_err());
    assert_eq!(fs::read(&path)?, original);
    Ok(())
}

#[test]
fn failed_replacement_preserves_original_and_cleans_temporary_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("account.json");
    fs::write(&path, b"original")?;
    let result = with_writer_lock(&path, || {
        replace_with(&path, |file| {
            file.write_all(b"partial replacement")?;
            anyhow::bail!("injected write failure")
        })
    });
    assert!(result.is_err());
    assert_eq!(fs::read(&path)?, b"original");
    assert!(!fs::read_dir(dir.path())?.any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(TEMP_PREFIX)));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_replacement_preserves_permissions() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("account.json");
    fs::write(&path, b"old")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    write(&path, b"new".to_vec()).await?;
    assert_eq!(fs::read(&path)?, b"new");
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o640);
    Ok(())
}

#[tokio::test]
async fn append_handles_a_complete_record_without_a_final_newline() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log.jsonl");
    fs::write(&path, b"1")?;
    append_jsonl(&path, &[2, 3]).await?;
    assert_eq!(read_jsonl::<u32>(&path).await?, vec![1, 2, 3]);
    Ok(())
}

#[tokio::test]
async fn cancellation_does_not_release_an_active_transaction_lock() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log.jsonl");
    append_jsonl(&path, &[0u32]).await?;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first_path = path.clone();
    let task = tokio::spawn(async move {
        update_jsonl(&first_path, move |items: &mut Vec<u32>| {
            items.push(1);
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(10))?;
            Ok(((), true))
        })
        .await
    });
    started_rx.await?;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join(".keepbook-lock-log.jsonl.lock"))?;
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    release_tx.send(())?;
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        update_jsonl(&path, |items: &mut Vec<u32>| {
            items.push(2);
            Ok(((), true))
        }),
    )
    .await??;
    assert_eq!(read_jsonl::<u32>(&path).await?, vec![0, 1, 2]);
    Ok(())
}

#[tokio::test]
async fn serialization_failure_does_not_append_a_partial_batch() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("log.jsonl");
    fs::write(&path, b"0\n")?;
    let valid = std::collections::HashMap::<Vec<u32>, u32>::new();
    let invalid = std::collections::HashMap::from([(vec![1], 2)]);
    assert!(append_jsonl(&path, &[valid, invalid]).await.is_err());
    assert_eq!(fs::read(&path)?, b"0\n");
    Ok(())
}
