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
