use super::*;

#[test]
fn transient_execution_context_errors_are_retryable() {
    assert!(is_transient_execution_context_error(
        "Error -32000: Cannot find context with specified id"
    ));
    assert!(is_transient_execution_context_error(
        "Execution context was destroyed, most likely because of a navigation"
    ));
    assert!(is_transient_execution_context_error(
        "Inspected target navigated or closed"
    ));
    assert!(!is_transient_execution_context_error(
        "Error -32601: Method not found"
    ));
}
