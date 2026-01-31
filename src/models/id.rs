use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Opaque identifier for stored entities.
///
/// For file-backed storage, ids should be safe path segments (no slashes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
