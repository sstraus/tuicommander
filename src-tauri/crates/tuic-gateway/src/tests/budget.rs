use std::sync::Arc;
use std::time::Duration;

use url::Url;

use crate::budget::{BucketConfig, BudgetBook, LocalLimit};
use crate::model::{AccountKey, BucketId};
use crate::ports::Clock;
use crate::testing::FakeClock;

fn setup() -> (BudgetBook, FakeClock, AccountKey) {
    let clock = FakeClock::new();
    let book = BudgetBook::new(Arc::new(clock.clone()));
    let account = AccountKey {
        account_id: "me".into(),
        api_base: Url::parse("https://api.github.com").unwrap(),
    };
    (book, clock, account)
}

fn id(s: &str) -> BucketId {
    BucketId(s.into())
}

fn per_minute(capacity: u32) -> BucketConfig {
    BucketConfig {
        local: Some(LocalLimit {
            capacity,
            per: Duration::from_secs(60),
        }),
        write_gap: None,
    }
}

#[test]
fn search_bucket_admits_30_per_minute_without_headers() {
    let (book, clock, acct) = setup();
    let search = id("search");
    book.configure(&acct, &search, per_minute(30));
    for _ in 0..30 {
        let _ = book.reserve(&acct, &search, 1).unwrap();
    }
    let denied = book.reserve(&acct, &search, 1).unwrap_err();
    assert_eq!(denied.retry_after, Some(Duration::from_secs(2)));
    clock.advance(Duration::from_secs(2));
    assert!(book.reserve(&acct, &search, 1).is_ok());
    assert!(book.reserve(&acct, &search, 1).is_err());
}

#[test]
fn header_remaining_overrides_local_estimate() {
    let (book, clock, acct) = setup();
    let b = id("core");
    book.configure(&acct, &b, per_minute(30));
    for _ in 0..5 {
        let r = book.reserve(&acct, &b, 1).unwrap();
        book.complete(r);
    }
    assert_eq!(book.status(&acct, &b).remaining, Some(25));
    let reset = clock.wall() + Duration::from_secs(30);
    book.update(&acct, &b, Some(5000), Some(0), Some(reset));
    let s = book.status(&acct, &b);
    assert_eq!((s.limit, s.remaining), (Some(5000), Some(0)));
    let denied = book.reserve(&acct, &b, 1).unwrap_err();
    assert_eq!(denied.retry_after, Some(Duration::from_secs(30)));
    // After the window resets the stale header no longer blocks.
    clock.advance(Duration::from_secs(31));
    assert!(book.reserve(&acct, &b, 1).is_ok());
}

#[test]
fn in_flight_units_are_reserved_before_send() {
    let (book, clock, acct) = setup();
    let b = id("core");
    let reset = clock.wall() + Duration::from_secs(60);
    book.update(&acct, &b, Some(100), Some(2), Some(reset));
    let first = book.reserve(&acct, &b, 1).unwrap();
    let _second = book.reserve(&acct, &b, 1).unwrap();
    assert!(book.reserve(&acct, &b, 1).is_err());
    assert_eq!(book.status(&acct, &b).in_flight, 2);
    book.cancel(first);
    assert!(book.reserve(&acct, &b, 1).is_ok());
}

#[test]
fn writes_are_fifo_and_one_second_apart_on_github() {
    let (book, clock, acct) = setup();
    let b = id("write");
    let cfg = BucketConfig {
        local: None,
        write_gap: Some(Duration::from_secs(1)),
    };
    book.configure(&acct, &b, cfg);
    let secs: Vec<_> = (0..3)
        .map(|_| book.reserve(&acct, &b, 1).unwrap().delay.as_secs())
        .collect();
    assert_eq!(secs, [0, 1, 2]);
    clock.advance(Duration::from_secs(10));
    assert_eq!(book.reserve(&acct, &b, 1).unwrap().delay, Duration::ZERO);
}
