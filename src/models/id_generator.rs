use std::collections::VecDeque;
use std::sync::Mutex;

use super::Id;

/// Abstraction over ID generation to support deterministic tests.
pub trait IdGenerator: Send + Sync {
    fn new_id(&self) -> Id;
}

#[derive(Debug, Clone, Default)]
pub struct UuidIdGenerator;

impl IdGenerator for UuidIdGenerator {
    fn new_id(&self) -> Id {
        Id::new()
    }
}

/// A deterministic generator that returns a pre-seeded sequence of IDs.
///
/// Panics if you request more IDs than provided.
#[derive(Debug, Default)]
pub struct FixedIdGenerator {
    ids: Mutex<VecDeque<Id>>,
}

impl FixedIdGenerator {
    pub fn new(ids: impl IntoIterator<Item = Id>) -> Self {
        Self {
            ids: Mutex::new(ids.into_iter().collect()),
        }
    }
}

impl IdGenerator for FixedIdGenerator {
    fn new_id(&self) -> Id {
        self.ids
            .lock()
            .expect("fixed id generator lock poisoned")
            .pop_front()
            .expect("fixed id generator exhausted")
    }
}
