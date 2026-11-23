use std::sync::Arc;
use std::time::{Duration, SystemTime};

use url::Url;

use crate::failure::{FailureMemory, FixedJitter, LimitHit, LimitKey, SeededJitter, single_retry};
use crate::model::{AccountKey, BucketId, Channel, Effect, LimitReason};
use crate::ports::Clock;
use crate::testing::FakeClock;

fn key() -> LimitKey {
    (
        AccountKey {
            account_id: "a".into(),
            api_base: Url::parse("https://api.github.com").unwrap(),
        },
        BucketId("core".into()),
    )
}

fn memory(clock: &FakeClock, jitter: f64) -> FailureMemory {
    FailureMemory::new(Arc::new(clock.clone()), Arc::new(FixedJitter(jitter)))
}

fn hit(retry_after: Option<u64>, reset: Option<SystemTime>, reason: LimitReason) -> LimitHit {
    LimitHit {
        retry_after: retry_after.map(Duration::from_secs),
        reset,
        reason,
    }
}

#[test]
fn retry_after_wins_over_reset_and_default() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    let reset = clock.wall() + Duration::from_secs(600);
    m.record_limit(&key(), hit(Some(7), Some(reset), LimitReason::Primary));
    assert_eq!(m.limit_wait(&key()), Some(Duration::from_secs(7)));
}

#[test]
fn reset_header_wins_over_default() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    let reset = clock.wall() + Duration::from_secs(300);
    m.record_limit(&key(), hit(None, Some(reset), LimitReason::Primary));
    assert_eq!(m.limit_wait(&key()), Some(Duration::from_secs(300)));
}

#[test]
fn github_secondary_without_headers_waits_sixty_seconds() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    let at = m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
    assert_eq!(m.limit_wait(&key()), Some(Duration::from_secs(60)));
    assert_eq!(at, clock.wall() + Duration::from_secs(60));
    clock.advance(Duration::from_secs(61));
    assert_eq!(m.limit_wait(&key()), None);
}

#[test]
fn repeated_limits_back_off_with_jitter_inside_bounds() {
    let clock = FakeClock::new();
    let m = FailureMemory::new(Arc::new(clock.clone()), Arc::new(SeededJitter::new(42)));
    let mut last = Duration::ZERO;
    for n in 0..12i32 {
        m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
        let secs = m.limit_wait(&key()).unwrap().as_secs_f64();
        let nominal = 60.0 * 2f64.powi(n);
        let (lo, hi) = if n == 0 {
            (60.0, 60.0)
        } else {
            ((nominal * 0.7).min(900.0), (nominal * 1.3).min(900.0))
        };
        assert!(secs >= lo - 0.01 && secs <= hi + 0.01, "n={n} wait={secs}");
        last = Duration::from_secs_f64(secs);
    }
    assert_eq!(last, Duration::from_secs(900), "capped at 15 min");
}

#[test]
fn jitter_scales_between_seventy_and_one_hundred_thirty_percent() {
    let clock = FakeClock::new();
    for (unit, expect) in [(0.0, 84.0), (1.0, 156.0)] {
        let m = memory(&clock, unit);
        m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
        m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
        let w = m.limit_wait(&key()).unwrap().as_secs_f64();
        assert!((w - expect).abs() < 0.01, "unit={unit} w={w}");
    }
}

#[test]
fn success_clears_limit_and_back_off_count() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
    m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
    m.record_success(&key(), "api.github.com");
    assert_eq!(m.limit_wait(&key()), None);
    m.record_limit(&key(), hit(None, None, LimitReason::Secondary));
    assert_eq!(m.limit_wait(&key()), Some(Duration::from_secs(60)));
}

#[test]
fn write_is_never_retried() {
    for channel in [Channel::Interactive, Channel::Visible, Channel::Background] {
        assert_eq!(
            single_retry(Effect::Write, channel, Some(Duration::from_secs(1)), false),
            None
        );
    }
}

#[test]
fn read_retries_once_only_when_interactive_and_retry_after_is_short() {
    let r = |ch, ra: Option<u64>, again| {
        single_retry(Effect::Read, ch, ra.map(Duration::from_secs), again)
    };
    assert_eq!(
        r(Channel::Interactive, Some(5), false),
        Some(Duration::from_secs(5))
    );
    assert_eq!(r(Channel::Interactive, Some(6), false), None);
    assert_eq!(r(Channel::Interactive, None, false), None);
    assert_eq!(r(Channel::Interactive, Some(1), true), None);
    assert_eq!(r(Channel::Visible, Some(1), false), None);
    assert_eq!(r(Channel::Background, Some(1), false), None);
}

#[test]
fn transport_failures_open_host_after_three() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    let host = "api.github.com";
    m.record_transport_failure(host);
    m.record_transport_failure(host);
    assert_eq!(m.host_wait(host), None);
    m.record_transport_failure(host);
    assert_eq!(m.host_wait(host), Some(Duration::from_secs(5)));
    clock.advance(Duration::from_secs(6));
    assert_eq!(m.host_wait(host), None);
    m.record_transport_failure(host);
    assert_eq!(m.host_wait(host), Some(Duration::from_secs(10)));
    assert_eq!(m.host_wait("gitlab.com"), None);
}

#[test]
fn transport_back_off_is_capped_at_five_minutes() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    for _ in 0..30 {
        m.record_transport_failure("h");
    }
    assert_eq!(m.host_wait("h"), Some(Duration::from_secs(300)));
}

#[test]
fn transport_success_closes_the_host() {
    let clock = FakeClock::new();
    let m = memory(&clock, 0.5);
    for _ in 0..3 {
        m.record_transport_failure("h");
    }
    m.record_success(&key(), "h");
    assert_eq!(m.host_wait("h"), None);
}
