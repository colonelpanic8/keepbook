use super::*;

#[test]
fn test_parse_entry() {
    let content = "mysecretpassword\nkey-name: organizations/abc\nprivate-key: -----BEGIN EC PRIVATE KEY-----\\nMIGk...\\n-----END EC PRIVATE KEY-----";

    let entry = FieldEntry::parse(content);

    assert_eq!(entry.password, Some("mysecretpassword".to_string()));
    assert_eq!(
        entry.fields.get("password"),
        Some(&"mysecretpassword".to_string())
    );
    assert_eq!(
        entry.fields.get("key-name"),
        Some(&"organizations/abc".to_string())
    );
    assert!(entry
        .fields
        .get("private-key")
        .unwrap()
        .contains("BEGIN EC PRIVATE KEY"));
    assert!(entry.fields.get("private-key").unwrap().contains('\n'));
}

#[test]
fn test_field_name_mapping() {
    let mut fields = HashMap::new();
    fields.insert("key_name".to_string(), "key-name".to_string());

    let store = PassCredentialStore::new(PassConfig {
        path: "test".to_string(),
        fields,
    });

    assert_eq!(store.field_name("key_name"), "key-name");
    assert_eq!(store.field_name("other"), "other");
}

#[test]
fn test_entry_roundtrip() {
    let content = "mysecret\napi-key: abc123\ntoken: xyz789";
    let entry = FieldEntry::parse(content);
    let serialized = entry.to_string();

    // Parse again and verify
    let reparsed = FieldEntry::parse(&serialized);
    assert_eq!(reparsed.password, entry.password);
    assert_eq!(reparsed.fields.get("api-key"), entry.fields.get("api-key"));
    assert_eq!(reparsed.fields.get("token"), entry.fields.get("token"));
}
