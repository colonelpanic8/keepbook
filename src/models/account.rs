use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::clock::{Clock, SystemClock};

use super::Id;
use super::{IdGenerator, UuidIdGenerator};
use crate::duration::deserialize_duration_opt;

/// Policy for handling balances before the first snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceBackfillPolicy {
    /// Exclude the account when no balance exists for the date.
    None,
    /// Show the account with a zero value when no balance exists for the date.
    Zero,
    /// Carry back the earliest balance snapshot to earlier dates.
    CarryEarliest,
}

/// An individual financial account (checking, savings, credit card, brokerage, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Id,
    pub name: String,
    pub connection_id: Id,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub active: bool,
    /// Opaque data owned by the synchronizer plugin
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl Account {
    pub fn new(name: impl Into<String>, connection_id: Id) -> Self {
        Self::new_with_generator(&UuidIdGenerator, &SystemClock, name, connection_id)
    }

    pub fn new_with(id: Id, created_at: DateTime<Utc>, name: impl Into<String>, connection_id: Id) -> Self {
        Self {
            id,
            name: name.into(),
            connection_id,
            tags: Vec::new(),
            created_at,
            active: true,
            synchronizer_data: serde_json::Value::Null,
        }
    }

    pub fn new_with_generator(
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
        name: impl Into<String>,
        connection_id: Id,
    ) -> Self {
        Self::new_with(ids.new_id(), clock.now(), name, connection_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::models::{FixedIdGenerator, Id};
    use chrono::TimeZone;

    #[test]
    fn account_new_with_generator_is_deterministic() {
        let fixed_id = Id::from_string("acct-1");
        let ids = FixedIdGenerator::new([fixed_id.clone()]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account = Account::new_with_generator(&ids, &clock, "Checking", Id::from_string("c"));
        assert_eq!(account.id, fixed_id);
        assert_eq!(account.created_at, clock.now());
    }
}

/// Optional account configuration (stored in account_config.toml).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountConfig {
    /// Override balance staleness for this account.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_duration_opt"
    )]
    pub balance_staleness: Option<std::time::Duration>,

    /// How to handle portfolio queries before the first balance snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_backfill: Option<BalanceBackfillPolicy>,
}
