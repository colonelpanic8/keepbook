use super::*;
use std::path::Path;

#[test]
fn relative_paths_resolve_against_base_dir() {
    let store = AgeCredentialStore::with_base_dir(
        AgeConfig {
            path: "creds/foo.age".to_string(),
            identity_path: Some("key".to_string()),
            fields: HashMap::new(),
        },
        Path::new("/tmp/keepbook"),
    );
    assert_eq!(
        store.resolve_path("creds/foo.age"),
        Path::new("/tmp/keepbook/creds/foo.age")
    );
}

#[test]
fn configured_identity_path_wins() -> Result<()> {
    let store = AgeCredentialStore::with_base_dir(
        AgeConfig {
            path: "creds/foo.age".to_string(),
            identity_path: Some("key".to_string()),
            fields: HashMap::new(),
        },
        Path::new("/tmp/keepbook"),
    );
    assert_eq!(
        store.configured_identity_path()?,
        Some(Path::new("/tmp/keepbook/key").to_path_buf())
    );
    Ok(())
}
