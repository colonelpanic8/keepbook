use super::*;
use crate::clock::FixedClock;
use crate::models::{FixedIdGenerator, Id};
use chrono::TimeZone;

#[test]
fn connection_state_new_with_generator_is_deterministic() {
    let fixed_id = Id::from_string("conn-1");
    let ids = FixedIdGenerator::new([fixed_id.clone()]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

    let state = ConnectionState::new_with_generator(&ids, &clock);
    assert_eq!(state.id, fixed_id);
    assert_eq!(state.created_at, clock.now());
}
