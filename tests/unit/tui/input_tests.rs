use super::*;
#[test]
fn filtered_tag_suggestions_prefers_prefix_matches() {
    let catalog = vec![
        "Groceries".to_string(),
        "Coffee".to_string(),
        "Dining Out".to_string(),
        "Office Coffee".to_string(),
    ];
    let out = filtered_tag_suggestions(&catalog, "cof");
    assert_eq!(
        out,
        vec!["Coffee".to_string(), "Office Coffee".to_string(),]
    );
}
#[test]
fn selected_tag_from_modal_uses_active_selection() {
    let modal = TagModalState {
        action: TagAction::OneOff {
            source: SelectedTransactionInfo {
                account_id: "acct-1".to_string(),
                account_name: "Checking".to_string(),
                transaction_id: "tx-1".to_string(),
                status: "posted".to_string(),
                amount: "-1".to_string(),
                description: "Coffee".to_string(),
            },
        },
        input: "din".to_string(),
        cursor: 3,
        suggestions: vec!["Dining".to_string()],
        selected_suggestion: 0,
        selection_active: true,
    };
    assert_eq!(selected_tag_from_modal(&modal), "Dining".to_string());
}
#[test]
fn apply_text_input_edit_supports_cursor_navigation_and_insert_delete() {
    let mut input = "abc".to_string();
    let mut cursor = 3usize;

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Left
    ));
    assert_eq!(cursor, 2);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Char('X')
    ));
    assert_eq!(input, "abXc");
    assert_eq!(cursor, 3);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Delete
    ));
    assert_eq!(input, "abX");
    assert_eq!(cursor, 3);

    assert!(apply_text_input_edit(
        &mut input,
        &mut cursor,
        KeyCode::Backspace
    ));
    assert_eq!(input, "ab");
    assert_eq!(cursor, 2);
}
