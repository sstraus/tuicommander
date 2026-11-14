use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::*;

const CADENCE_MS: u64 = 300_000;
const T0: u64 = 1_794_450_000_000;

struct ManualClock(AtomicU64);

impl ManualClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Owner double: counts active reads, answers from a script, and can be
/// slowed down so concurrent callers really overlap.
struct FakeOwner {
    clock: Arc<ManualClock>,
    reads: AtomicUsize,
    answers: parking_lot::Mutex<VecDeque<Result<(), QuotaError>>>,
    observed: parking_lot::Mutex<Option<QuotaSnapshot>>,
    fingerprint: parking_lot::Mutex<Option<String>>,
    delay: Duration,
}

impl FakeOwner {
    fn new(clock: Arc<ManualClock>) -> Self {
        Self {
            clock,
            reads: AtomicUsize::new(0),
            answers: Default::default(),
            observed: Default::default(),
            fingerprint: parking_lot::Mutex::new(Some("login-1".into())),
            delay: Duration::ZERO,
        }
    }

    fn answer(&self, result: Result<(), QuotaError>) {
        self.answers.lock().push_back(result);
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

impl QuotaOwner for FakeOwner {
    fn owner(&self) -> &'static str {
        "fake"
    }

    fn observed(&self, _key: &SubscriptionKey) -> Option<QuotaSnapshot> {
        self.observed.lock().clone()
    }

    fn read<'a>(
        &'a self,
        _key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            let answer = self.answers.lock().pop_front().unwrap_or(Ok(()));
            answer.map(|()| snapshot(self.clock.now_ms(), 40.0, QuotaSource::Active))
        })
    }

    fn credential_fingerprint(&self, _key: &SubscriptionKey) -> Option<Fingerprint> {
        self.fingerprint.lock().clone()
    }
}

fn snapshot(observed_at_ms: u64, used: f64, source: QuotaSource) -> QuotaSnapshot {
    QuotaSnapshot {
        windows: vec![QuotaWindow::from_duration(used, Some(18_000), None, None).unwrap()],
        detail: None,
        plan: Some("pro".into()),
        credits: None,
        account: None,
        observed_at_ms,
        source,
    }
}

fn key(credential: &str) -> SubscriptionKey {
    SubscriptionKey::new("fake", "anthropic", credential)
}

fn setup(owner: impl FnOnce(&mut FakeOwner)) -> (Arc<ManualClock>, Arc<FakeOwner>, QuotaRegistry) {
    let clock = Arc::new(ManualClock(AtomicU64::new(T0)));
    let mut fake = FakeOwner::new(clock.clone());
    owner(&mut fake);
    let fake = Arc::new(fake);
    let registry = QuotaRegistry::new(clock.clone(), QuotaConfig::default());
    registry.register_owner(fake.clone());
    (clock, fake, registry)
}

/// Catches: missing single-flight — ticker, dashboard and mobile each read on a cold key.
#[tokio::test]
async fn cold_key_with_concurrent_readers_fetches_once() {
    let (_, owner, registry) = setup(|o| o.delay = Duration::from_millis(30));
    let k = key("~/.claude-private");
    let views = futures_util::future::join_all((0..8).map(|_| registry.ensure(&k))).await;
    assert_eq!(owner.reads(), 1);
    assert!(
        views.iter().all(|v| v.state == QuotaState::Fresh),
        "{views:?}"
    );
}

/// Catches: network reads while the harness already reports a current value.
#[tokio::test]
async fn fresh_passive_value_suppresses_active_read() {
    let (_, owner, registry) = setup(|o| {
        *o.observed.get_mut() = Some(snapshot(T0 - 1_000, 13.0, QuotaSource::Passive));
    });
    let view = registry.ensure(&key("~/.codex")).await;
    assert_eq!(owner.reads(), 0);
    assert_eq!(view.state, QuotaState::Fresh);
    assert_eq!(view.snapshot.unwrap().source, QuotaSource::Passive);
}

/// Catches: no fresh value after the harness has been idle for a long time.
#[tokio::test]
async fn stale_key_of_idle_harness_triggers_one_active_read() {
    let (_, owner, registry) = setup(|o| {
        *o.observed.get_mut() = Some(snapshot(T0 - 2 * CADENCE_MS, 13.0, QuotaSource::Passive));
    });
    registry.bind("tab-1", key("~/.codex"));
    registry.tick().await;
    registry.tick().await;
    assert_eq!(owner.reads(), 1);
    let view = registry.get(&key("~/.codex"));
    assert_eq!(view.state, QuotaState::Fresh);
    assert_eq!(view.snapshot.unwrap().source, QuotaSource::Active);
}

/// Catches: 401 hammering — an expired login read on every pass (2026-11-12 ego badge).
#[tokio::test]
async fn auth_failure_blocks_reads_until_credential_changes() {
    let (clock, owner, registry) = setup(|_| {});
    owner.answer(Err(QuotaError::Auth("401".into())));
    let k = key("default");
    assert_eq!(registry.ensure(&k).await.state, QuotaState::AuthInvalid);
    for _ in 0..5 {
        clock.advance(CADENCE_MS);
        registry.ensure(&k).await;
    }
    assert_eq!(
        owner.reads(),
        1,
        "no read while the same credential is invalid"
    );
    *owner.fingerprint.lock() = Some("login-2".into());
    assert_eq!(registry.ensure(&k).await.state, QuotaState::Fresh);
    assert_eq!(owner.reads(), 2);
}

/// Catches: a blank badge, or a retry inside the server's retry-after, after a 429.
#[tokio::test]
async fn rate_limit_keeps_last_value_and_honours_retry_after() {
    let (clock, owner, registry) = setup(|_| {});
    let k = key("default");
    registry.ensure(&k).await;
    clock.advance(CADENCE_MS);
    owner.answer(Err(QuotaError::RateLimited {
        retry_after_secs: Some(600),
    }));
    let view = registry.ensure(&k).await;
    let until_ms = clock.now_ms() + 600_000;
    assert_eq!(view.state, QuotaState::Backoff { until_ms });
    assert!(view.snapshot.is_some(), "last good value is kept");
    clock.advance(599_000);
    registry.ensure(&k).await;
    assert_eq!(owner.reads(), 2);
    clock.advance(2_000);
    registry.ensure(&k).await;
    assert_eq!(owner.reads(), 3);
}

/// Catches: background reads for keys whose tabs are closed.
#[tokio::test]
async fn unbound_key_is_not_scheduled() {
    let (clock, owner, registry) = setup(|_| {});
    registry.observe(
        &key("other"),
        snapshot(T0 - 2 * CADENCE_MS, 1.0, QuotaSource::Passive),
    );
    registry.bind("tab-1", key("bound"));
    registry.tick().await;
    assert_eq!(owner.reads(), 1, "only the bound key is read");
    registry.unbind("tab-1");
    clock.advance(CADENCE_MS);
    registry.tick().await;
    assert_eq!(owner.reads(), 1);
}

/// Catches: an older passive value (old rollout) replacing a newer read.
#[tokio::test]
async fn older_observation_never_replaces_a_newer_value() {
    let (_, _, registry) = setup(|_| {});
    let k = key("~/.codex");
    assert!(registry.observe(&k, snapshot(T0, 20.0, QuotaSource::Active)));
    assert!(!registry.observe(&k, snapshot(T0 - 60_000, 5.0, QuotaSource::Passive)));
    let used = registry.get(&k).snapshot.unwrap().windows[0].used_percent;
    assert!((used - 20.0).abs() < f64::EPSILON);
}

/// Catches: a changed response schema still shown as the old number.
#[tokio::test]
async fn contract_error_hides_the_stale_value() {
    let (clock, owner, registry) = setup(|_| {});
    let k = key("default");
    registry.ensure(&k).await;
    clock.advance(CADENCE_MS);
    owner.answer(Err(QuotaError::Contract("missing five_hour".into())));
    let view = registry.ensure(&k).await;
    assert!(view.snapshot.is_none());
    assert!(matches!(view.state, QuotaState::ContractError { .. }));
}

/// Catches: a weekly window shown as five-hour because a provider calls it "primary".
/// Codex reported its 10080-minute window as `primary` on 2026-11-12.
#[test]
fn windows_are_classified_by_duration_not_field_name() {
    let codex_primary = QuotaWindow::from_duration(13.0, Some(10_080 * 60), None, None).unwrap();
    assert_eq!(
        (codex_primary.kind, codex_primary.label.as_str()),
        (WindowKind::Weekly, "7d")
    );
    for (seconds, kind) in [
        (3 * HOUR, WindowKind::FiveHour),
        (8 * HOUR, WindowKind::FiveHour),
        (8 * HOUR + 1, WindowKind::Other),
        (5 * DAY, WindowKind::Weekly),
        (9 * DAY + 1, WindowKind::Other),
        (30 * DAY, WindowKind::Monthly),
    ] {
        assert_eq!(classify(Some(seconds)), kind, "{seconds} s");
    }
    let credits = QuotaWindow::from_duration(26.0, None, None, None).unwrap();
    assert_eq!(
        (credits.kind, credits.label.as_str()),
        (WindowKind::Other, "credits")
    );
    assert!(QuotaWindow::from_duration(f64::NAN, Some(18_000), None, None).is_none());
}

/// Catches a newly bound or unbound session remaining stale in frontend stores.
#[test]
fn binding_changes_publish_in_order_without_duplicates() {
    let (_, _, registry) = setup(|_| {});
    let mut changes = registry.subscribe_bindings();
    let first = key("first");
    let second = key("second");
    registry.bind("session", first.clone());
    assert_eq!(registry.get(&first).session_ids, vec!["session"]);
    registry.bind("session", first.clone());
    registry.bind("session", second.clone());
    registry.unbind("session");
    assert_eq!(registry.get(&second).session_ids, Vec::<String>::new());
    registry.unbind("session");
    assert_eq!(changes.try_recv().unwrap().key, Some(first));
    assert_eq!(changes.try_recv().unwrap().key, Some(second));
    assert_eq!(changes.try_recv().unwrap().key, None);
    assert!(changes.try_recv().is_err());
}

// Catches a new passive window discarding independently timed active details.
#[test]
fn passive_snapshot_keeps_previous_active_detail() {
    let key = key("detail-merge");
    let registry = QuotaRegistry::default();
    let mut active = snapshot(100, 41.0, QuotaSource::Active);
    let response: crate::claude_usage::UsageApiResponse =
        serde_json::from_str(include_str!("fixtures/anthropic-usage.json")).unwrap();
    active.detail = Some(QuotaDetail::Claude {
        observed_at_ms: 100,
        response,
    });
    assert!(registry.observe(&key, active));
    assert!(registry.observe(&key, snapshot(200, 42.0, QuotaSource::Passive)));
    let current = registry.get(&key).snapshot.unwrap();
    assert_eq!(current.windows[0].used_percent, 42.0);
    assert!(matches!(
        current.detail,
        Some(QuotaDetail::Claude {
            observed_at_ms: 100,
            ..
        })
    ));
}

// Catches an active read arriving after a newer passive window losing its detail.
#[test]
fn older_window_can_supply_newer_active_detail() {
    let key = key("detail-order");
    let registry = QuotaRegistry::default();
    registry.observe(&key, snapshot(300, 42.0, QuotaSource::Passive));
    let mut active = snapshot(200, 41.0, QuotaSource::Active);
    active.detail = Some(QuotaDetail::Claude {
        observed_at_ms: 200,
        response: serde_json::from_str(include_str!("fixtures/anthropic-usage.json")).unwrap(),
    });
    assert!(registry.observe(&key, active.clone()));
    let current = registry.get(&key).snapshot.unwrap();
    assert_eq!(current.windows[0].used_percent, 42.0);
    assert_eq!(current.detail, active.detail);
    active.detail = Some(QuotaDetail::Claude {
        observed_at_ms: 100,
        response: crate::claude_usage::UsageApiResponse::default(),
    });
    assert!(!registry.observe(&key, active));
    assert_eq!(registry.get(&key).snapshot.unwrap().detail, current.detail);
}

fn recorded_quota() -> crate::codex_usage::CodexUsageApiResponse {
    let mut body: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/openai-codex-usage.json")).unwrap();
    body["additional_rate_limits"] = serde_json::json!([]);
    serde_json::from_value(body).unwrap()
}

// Catches dashboard/session callers bypassing the shared default credential
// owner and losing flags that are not represented by normalized windows.
#[tokio::test]
async fn registry_and_codex_consumers_share_details() {
    use crate::codex_usage::{CodexUsageApiResponse, session_quota};
    use std::sync::Arc;
    struct Owner(Arc<AtomicUsize>);
    impl QuotaOwner for Owner {
        fn owner(&self) -> &'static str {
            "codex-cli"
        }
        fn unavailable_reason(&self, key: &SubscriptionKey) -> Option<&'static str> {
            key.profile.as_ref().map(|_| "profile not supported")
        }
        fn read<'a>(
            &'a self,
            _: &'a SubscriptionKey,
        ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                let response: CodexUsageApiResponse = recorded_quota();
                let now = SystemClock.now_ms();
                Ok(QuotaSnapshot {
                    windows: vec![],
                    detail: Some(QuotaDetail::Codex {
                        observed_at_ms: now,
                        response,
                    }),
                    plan: None,
                    credits: None,
                    account: None,
                    observed_at_ms: now,
                    source: QuotaSource::Active,
                })
            })
        }
    }
    let registry = QuotaRegistry::default();
    let calls = Arc::new(AtomicUsize::new(0));
    registry.register_owner(Arc::new(Owner(calls.clone())));
    let key = SubscriptionKey::new("codex-cli", "openai-codex", "/fixture/codex");
    registry.ensure(&key).await;
    let expected: CodexUsageApiResponse = recorded_quota();
    assert_eq!(
        session_quota(&registry, "/fixture/codex", None)
            .await
            .unwrap(),
        expected
    );
    assert_eq!(
        session_quota(&registry, "/fixture/codex", None)
            .await
            .unwrap(),
        expected
    );
    let error = session_quota(&registry, "/fixture/codex", Some("work"))
        .await
        .unwrap_err();
    assert!(error.contains("profile not supported"), "{error}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
