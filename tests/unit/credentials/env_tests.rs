use super::*;
use secrecy::ExposeSecret;

#[tokio::test]
async fn mapped_env_var_is_read() -> Result<()> {
    std::env::set_var("KEEPBOOK_TEST_TOKEN", "secret");
    let mut fields = HashMap::new();
    fields.insert("token".to_string(), "KEEPBOOK_TEST_TOKEN".to_string());
    let store = EnvCredentialStore::new(EnvConfig {
        prefix: None,
        fields,
    });

    let value = store.get("token").await?.expect("token");
    assert_eq!(value.expose_secret(), "secret");
    assert!(!store.supports_write());
    Ok(())
}

#[test]
fn key_names_are_normalized_with_prefix() {
    let store = EnvCredentialStore::new(EnvConfig {
        prefix: Some("KEEPBOOK_".to_string()),
        fields: HashMap::new(),
    });
    assert_eq!(store.env_name("private-key"), "KEEPBOOK_PRIVATE_KEY");
}
