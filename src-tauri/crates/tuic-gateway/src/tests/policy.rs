use std::time::{Duration, SystemTime};

use crate::budget::BucketStatus;
use crate::model::{Channel, Freshness, Pressure};
use crate::policy::{
    Admission, BucketState, BurnTracker, CacheView, admit, pressure, ttl_multiplier,
};

const NOW: SystemTime = SystemTime::UNIX_EPOCH;
const LIMIT: u32 = 1000;

fn at(secs: u64) -> SystemTime {
    NOW + Duration::from_secs(secs)
}

fn state(remaining: u32) -> BucketState {
    BucketState {
        status: BucketStatus {
            limit: Some(LIMIT),
            remaining: Some(remaining),
            reset: Some(at(3600)),
            in_flight: 0,
        },
        burn_per_sec: 0.0,
        now: NOW,
        blocked_until: None,
    }
}

const MISS: CacheView = CacheView {
    fresh: false,
    stale: false,
    validator: false,
};
const STALE: CacheView = CacheView {
    fresh: false,
    stale: true,
    validator: true,
};
const FRESH: CacheView = CacheView {
    fresh: true,
    stale: false,
    validator: true,
};

use Admission::{Conditional as C, ServeCache as S};
use Channel::{Background as B, Interactive as I, Visible as V};

fn kind(a: Admission) -> &'static str {
    match a {
        Admission::Network => "Network",
        Admission::Conditional => "Conditional",
        Admission::ServeCache => "ServeCache",
        Admission::Shed(_) => "Shed",
        Admission::Limited(_) => "Limited",
    }
}

fn check(remaining: u32, cache: CacheView, f: Freshness, want: [&str; 3]) {
    for (ch, w) in [I, V, B].into_iter().zip(want) {
        let got = admit(&state(remaining), ch, f, cache);
        assert_eq!(kind(got), w, "remaining {remaining}/1000 {ch:?} {f:?}");
    }
}

#[test]
fn policy_table_stale_validator_entry_at_every_edge() {
    let d = Freshness::Default;
    check(600, STALE, d, ["Conditional", "Conditional", "Conditional"]);
    check(501, STALE, d, ["Conditional", "Conditional", "Conditional"]);
    check(500, STALE, d, ["Conditional", "ServeCache", "Conditional"]); // 50 % is Watch
    check(200, STALE, d, ["Conditional", "ServeCache", "Conditional"]); // 20 % is Watch
    check(199, STALE, d, ["Conditional", "Conditional", "Conditional"]);
    check(50, STALE, d, ["Conditional", "Conditional", "Conditional"]);
    check(49, STALE, d, ["Conditional", "ServeCache", "Shed"]); // Critical
    check(1, STALE, d, ["Conditional", "ServeCache", "Shed"]);
    check(0, STALE, d, ["ServeCache", "ServeCache", "ServeCache"]);
}

#[test]
fn policy_table_miss_without_cache() {
    let d = Freshness::Default;
    check(600, MISS, d, ["Network", "Network", "Network"]);
    check(500, MISS, d, ["Network", "Network", "Network"]);
    check(200, MISS, d, ["Network", "Network", "Network"]);
    check(199, MISS, d, ["Network", "Network", "Shed"]);
    check(49, MISS, d, ["Network", "Shed", "Shed"]);
    check(0, MISS, d, ["Limited", "Limited", "Limited"]);
}

#[test]
fn policy_fresh_entry_is_served_unless_exhausted_or_forced() {
    for r in [600, 500, 200, 49, 0] {
        check(r, FRESH, Freshness::Default, ["ServeCache"; 3]);
    }
}

#[test]
fn policy_force_acts_as_interactive_and_bypasses_freshness_above_l4() {
    for r in [600, 500, 200, 50, 49] {
        check(r, FRESH, Freshness::Force, ["Conditional"; 3]);
        check(r, MISS, Freshness::Force, ["Network"; 3]);
    }
}

#[test]
fn force_never_sends_into_exhausted_bucket() {
    check(0, MISS, Freshness::Force, ["Limited"; 3]);
    check(0, FRESH, Freshness::Force, ["ServeCache"; 3]);
    check(0, STALE, Freshness::Force, ["ServeCache"; 3]);
    // Failure memory is L4 too, whatever the header says.
    let mut s = state(900);
    s.blocked_until = Some(at(60));
    let a = admit(&s, I, Freshness::Force, MISS);
    assert_eq!(a, Admission::Limited(at(60)));
}

#[test]
fn policy_exhausted_bucket_retries_at_reset() {
    let a = admit(&state(0), I, Freshness::Default, MISS);
    assert_eq!(a, Admission::Limited(at(3600)));
}

#[test]
fn policy_elapsed_reset_means_refilled() {
    let mut s = state(0);
    s.status.reset = Some(NOW);
    assert_eq!(pressure(&s), Pressure::Free);
}

#[test]
fn policy_critical_allows_one_interactive_in_flight() {
    let mut s = state(10);
    s.status.in_flight = 1;
    assert!(matches!(
        admit(&s, I, Freshness::Default, MISS),
        Admission::Shed(_)
    ));
    assert!(matches!(
        admit(&s, I, Freshness::Force, STALE),
        Admission::Shed(_)
    ));
    assert_eq!(admit(&s, I, Freshness::Default, FRESH), S);
    s.status.in_flight = 0;
    assert_eq!(admit(&s, I, Freshness::Default, MISS), Admission::Network);
}

#[test]
fn policy_unknown_budget_is_free() {
    let mut s = state(0);
    s.status.limit = None;
    s.status.remaining = None;
    assert_eq!(pressure(&s), Pressure::Free);
    s.status.remaining = Some(0);
    assert_eq!(pressure(&s), Pressure::Exhausted);
}

#[test]
fn projection_raises_level_when_burn_empties_bucket_before_reset() {
    let mut s = state(400); // 40 % is Watch
    assert_eq!(pressure(&s), Pressure::Watch);
    s.burn_per_sec = 400.0 / 120.0; // empty in 2 min, reset in 1 h
    assert_eq!(pressure(&s), Pressure::Tight);
    s.burn_per_sec = 400.0 / 7200.0; // lasts past the reset
    assert_eq!(pressure(&s), Pressure::Watch);
}

#[test]
fn policy_projection_never_reaches_exhausted_and_needs_a_reset() {
    let mut s = state(30);
    s.burn_per_sec = 1000.0;
    assert_eq!(pressure(&s), Pressure::Critical);
    let mut s = state(400);
    s.burn_per_sec = 1000.0;
    s.status.reset = None;
    assert_eq!(pressure(&s), Pressure::Watch);
}

#[test]
fn free_quota_applies_no_extra_strictness() {
    let mut s = state(900);
    s.status.in_flight = 5;
    for ch in [I, V, B] {
        assert_eq!(pressure(&s), Pressure::Free);
        assert_eq!(admit(&s, ch, Freshness::Default, MISS), Admission::Network);
        assert_eq!(admit(&s, ch, Freshness::Default, STALE), C);
        assert_eq!(admit(&s, ch, Freshness::Default, FRESH), S);
    }
}

#[test]
fn policy_cache_only_never_reaches_the_network() {
    for r in [900, 300, 100, 10, 0] {
        for ch in [I, V, B] {
            assert_eq!(admit(&state(r), ch, Freshness::CacheOnly, MISS), S);
        }
    }
}

#[test]
fn policy_ttl_multiplier_follows_the_plan_table() {
    let m = ttl_multiplier;
    assert_eq!(m(Pressure::Free, B), 1);
    assert_eq!(m(Pressure::Watch, B), 2);
    assert_eq!(m(Pressure::Tight, B), 4);
    assert_eq!(m(Pressure::Tight, V), 4);
    assert_eq!(m(Pressure::Watch, V), 1);
    assert_eq!(m(Pressure::Tight, I), 1);
}

#[test]
fn burn_tracker_decays_with_five_minute_constant() {
    let mut t = BurnTracker::new();
    assert_eq!(t.rate(at(0)), 0.0);
    t.record(300, at(0));
    assert!((t.rate(at(0)) - 1.0).abs() < 1e-9);
    let later = t.rate(at(300));
    assert!((later - (-1.0f64).exp()).abs() < 1e-9, "{later}");
    t.record(300, at(300));
    assert!((t.rate(at(300)) - (later + 1.0)).abs() < 1e-9);
}
