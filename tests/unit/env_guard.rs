//! Shared guard for unit tests that mutate process-wide environment variables.
//!
//! Cargo runs a crate's tests on many threads of one process, so an unguarded
//! `set_var` is visible to every other test that reads the same variable. Every
//! such test must go through this one guard, or they serialize against
//! different locks and race anyway.
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the environment lock for its lifetime and puts back everything it
/// changed on drop, including when the test panics part way through.
///
/// Mutate only through [`EnvGuard::set`] and [`EnvGuard::remove`]; they record
/// the pre-test value on first touch, so there is no separate list to keep in
/// sync with what a test actually changes.
pub struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    pub fn new() -> Self {
        Self {
            // A test that panics while holding the lock poisons it but leaves
            // the environment restored by `drop`, so the poison is not a reason
            // to fail every later test.
            _lock: ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner()),
            saved: Vec::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        self.save(name);
        std::env::set_var(name, value);
    }

    pub fn remove(&mut self, name: &str) {
        self.save(name);
        std::env::remove_var(name);
    }

    fn save(&mut self, name: &str) {
        if !self.saved.iter().any(|(saved, _)| saved == name) {
            self.saved.push((name.to_string(), std::env::var_os(name)));
        }
    }
}

impl Default for EnvGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.saved.drain(..) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
