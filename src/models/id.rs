use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "Invalid id {value:?}: ids must be a single path segment (no '/', '\\\\', NUL, '.' or '..')"
)]
pub struct IdError {
    value: String,
}

/// Opaque identifier for stored entities.
///
/// For file-backed storage, ids should be safe path segments (no slashes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl Id {
    /// Namespace UUID for generating deterministic IDs from external identifiers.
    const NAMESPACE: Uuid = Uuid::from_u128(0x6ba7b810_9dad_11d1_80b4_00c04fd430c8);

    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create an ID from an arbitrary string.
    /// Note: The string must be a valid path segment (no slashes).
    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Create an ID from an arbitrary string, validating that it is a safe path segment.
    pub fn from_string_checked(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if Self::is_path_safe(&value) {
            Ok(Self(value))
        } else {
            Err(IdError { value })
        }
    }

    /// Create a deterministic, filesystem-safe ID from an external identifier.
    /// Uses UUID5 to hash the input, ensuring the same input always produces the same ID.
    /// This is useful for external IDs that may contain special characters (like base64).
    pub fn from_external(value: &str) -> Self {
        Self(Uuid::new_v5(&Self::NAMESPACE, value.as_bytes()).to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if the string is safe to use as a single path segment.
    pub fn is_path_safe(value: &str) -> bool {
        if value.is_empty() || value == "." || value == ".." {
            return false;
        }
        !value.chars().any(|c| c == '/' || c == '\\' || c == '\0')
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Id {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_external_is_deterministic() {
        let first = Id::from_external("schwab-account-123");
        let second = Id::from_external("schwab-account-123");
        assert_eq!(first, second);
    }

    #[test]
    fn test_from_external_differs_for_different_inputs() {
        let first = Id::from_external("schwab-account-123");
        let second = Id::from_external("schwab-account-456");
        assert_ne!(first, second);
    }

    #[test]
    fn test_from_external_is_path_safe() {
        let id = Id::from_external("weird/account/value");
        assert!(!id.as_str().contains('/'));
    }

    #[test]
    fn test_from_string_keeps_value() {
        let id = Id::from_string("account-id-123");
        assert_eq!(id.as_str(), "account-id-123");
    }

    #[test]
    fn test_from_string_checked_rejects_unsafe_values() {
        assert!(Id::from_string_checked("../escape").is_err());
        assert!(Id::from_string_checked("..").is_err());
        assert!(Id::from_string_checked(".").is_err());
        assert!(Id::from_string_checked("foo/bar").is_err());
        assert!(Id::from_string_checked("foo\\bar").is_err());
        assert!(Id::from_string_checked("bad\0id").is_err());
    }
}
