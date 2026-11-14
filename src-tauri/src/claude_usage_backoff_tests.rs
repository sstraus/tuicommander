use super::usage_state::*;
use super::*;
use futures_util::FutureExt;

// Catches a 429 on the usage endpoint spending the shared quota on the messages
// headers fallback (2819-2486: an OAuth token gets 401 there, live log 2026-11-04)
// or losing the last reading: the cooldown keeps it, and serves it without IO.
#[tokio::test]
async fn desktop_usage_429_keeps_cooldown_and_serves_last_reading_without_fallback() {
    let evidence: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/claude_usage_429.json")).unwrap();
    let profile = CredentialProfile::ConfigDir("/fixture/team-2685".into());
    cache_response(
        &profile,
        &UsageApiResponse {
            five_hour: Some(RateBucket {
                utilization: 41.0,
                resets_at: None,
            }),
            ..Default::default()
        },
    );
    API_CACHE.lock().get_mut(&profile).unwrap().fetched_at =
        Instant::now() - API_CACHE_TTL - Duration::from_secs(1);
    let response = usage_api_for_profile(
        &profile,
        |_| Ok((Some("test-only".into()), None)),
        |_| {
            std::future::ready(Err(FetchError {
                status: evidence["status"].as_u64().unwrap() as u16,
                message: "recorded throttle".into(),
                retry_after_secs: evidence["retry_after_secs"].as_u64(),
            }))
        },
    )
    .now_or_never()
    .expect("usage response must not sleep for the provider's Retry-After")
    .expect("a throttled usage endpoint must serve the last reading");
    assert_eq!(response.five_hour.unwrap().utilization, 41.0);
    assert!(response.stale_age_secs.is_some());
    let deadline = RATE_LIMITED_UNTIL.lock()[&profile];
    assert!(deadline.duration_since(Instant::now()) <= Duration::from_secs(300));

    let repeated = usage_api_for_profile(
        &profile,
        |_| panic!("cooldown must precede credential IO"),
        |_| -> std::future::Ready<Result<UsageApiResponse, FetchError>> {
            panic!("cooldown must suppress the primary request")
        },
    )
    .await
    .expect("cooldown must serve the cached reading");
    assert_eq!(repeated.five_hour.unwrap().utilization, 41.0);
    RATE_LIMITED_UNTIL.lock().remove(&profile);
    API_CACHE.lock().remove(&profile);
}

// Catches a throttled profile with no usable fallback losing its retry time.
#[tokio::test]
async fn desktop_usage_429_without_fallback_reports_retry_time() {
    let profile = CredentialProfile::ConfigDir("/fixture/no-fallback-2685".into());
    let error = usage_api_for_profile(
        &profile,
        |_| Ok((Some("test-only".into()), None)),
        |_| {
            std::future::ready(Err(FetchError {
                status: 429,
                message: String::new(),
                retry_after_secs: Some(3600),
            }))
        },
    )
    .now_or_never()
    .expect("429 must return without a timer")
    .unwrap_err();
    assert!(
        error.starts_with("Claude usage rate-limited, retry "),
        "{error}"
    );
    let repeated = usage_api_for_profile(
        &profile,
        |_| panic!("cooldown must precede credential IO"),
        |_| -> std::future::Ready<Result<UsageApiResponse, FetchError>> { panic!("no IO") },
    )
    .await
    .unwrap_err();
    assert!(repeated.starts_with("Claude usage rate-limited, retry "));
    RATE_LIMITED_UNTIL.lock().remove(&profile);
}

// Catches an oversized/missing hint stranding a profile, leaking its cooldown to
// another account, or retaining it after a successful refresh.
#[tokio::test]
async fn desktop_usage_backoff_is_bounded_isolated_and_expires() {
    for (index, hint, seconds) in [
        (0, None, 120),
        (1, Some(0), 120),
        (2, Some(30), 120),
        (3, Some(300), 300),
        (4, Some(u64::MAX), 300),
    ] {
        let profile = CredentialProfile::ConfigDir(format!("/fixture/backoff-2685-{index}"));
        let before = Instant::now();
        let error = usage_api_for_profile(
            &profile,
            |_| Ok((Some("test-only".into()), None)),
            |_| {
                std::future::ready(Err(FetchError {
                    status: 429,
                    message: String::new(),
                    retry_after_secs: hint,
                }))
            },
        )
        .now_or_never()
        .expect("429 must return without a timer")
        .unwrap_err();
        assert!(error.starts_with("Claude usage rate-limited, retry "));
        let after = Instant::now();
        let deadline = RATE_LIMITED_UNTIL.lock()[&profile];
        assert!(deadline >= before + Duration::from_secs(seconds));
        assert!(deadline <= after + Duration::from_secs(seconds));

        let other = CredentialProfile::ConfigDir(format!("/fixture/other-2685-{index}"));
        let read = |dir: Option<&str>| {
            assert!(dir.is_some());
            Ok((Some("test-only".into()), None))
        };
        let fetch = |_| {
            std::future::ready(Ok(UsageApiResponse {
                five_hour: Some(RateBucket {
                    utilization: 37.0,
                    resets_at: None,
                }),
                ..Default::default()
            }))
        };
        let usage = usage_api_for_profile(&other, read, fetch).await.unwrap();
        assert_eq!(usage.five_hour.unwrap().utilization, 37.0);
        RATE_LIMITED_UNTIL
            .lock()
            .insert(profile.clone(), Instant::now() - Duration::from_secs(1));
        let usage = usage_api_for_profile(&profile, read, fetch).await.unwrap();
        assert_eq!(usage.five_hour.unwrap().utilization, 37.0);
        assert!(!RATE_LIMITED_UNTIL.lock().contains_key(&profile));
        API_CACHE.lock().remove(&profile);
        API_CACHE.lock().remove(&other);
    }
}

// Catches losing the stale reading when a non-429 request fails.
#[tokio::test]
async fn desktop_usage_non_429_keeps_stale_fallback() {
    let profile = CredentialProfile::ConfigDir("/fixture/fallback-2685".into());
    cache_response(
        &profile,
        &UsageApiResponse {
            five_hour: Some(RateBucket {
                utilization: 23.0,
                resets_at: None,
            }),
            ..Default::default()
        },
    );
    API_CACHE.lock().get_mut(&profile).unwrap().fetched_at = Instant::now() - API_CACHE_TTL;
    let usage = usage_api_for_profile(
        &profile,
        |_| Ok((Some("test-only".into()), None)),
        |_| async {
            Err(FetchError {
                status: 401,
                message: "recorded invalid bearer response".into(),
                retry_after_secs: None,
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(usage.five_hour.unwrap().utilization, 23.0);
    assert!(!RATE_LIMITED_UNTIL.lock().contains_key(&profile));
    API_CACHE.lock().remove(&profile);
}

fn rate_limited(
    retry_after_secs: Option<u64>,
) -> impl FnOnce(String) -> std::future::Ready<Result<UsageApiResponse, FetchError>> {
    move |_| {
        std::future::ready(Err(FetchError {
            status: 429,
            message: "rate limited".into(),
            retry_after_secs,
        }))
    }
}

/// Catches a 429 still sending the headers fallback request, which an OAuth
/// token cannot satisfy and which spends the shared quota once more.
#[tokio::test]
async fn usage_429_sends_no_headers_fallback_and_serves_last_reading_with_age() {
    let none = CredentialProfile::ConfigDir("/fixture/usage-429-none-2819".into());
    let error = usage_api_for_profile(&none, |_| Ok((Some("t".into()), None)), rate_limited(None))
        .await
        .unwrap_err();
    assert!(
        error.starts_with("Claude usage rate-limited, retry "),
        "{error}"
    );

    let known = CredentialProfile::ConfigDir("/fixture/usage-429-known-2819".into());
    cache_response(
        &known,
        &UsageApiResponse {
            five_hour: Some(RateBucket {
                utilization: 37.0,
                resets_at: None,
            }),
            ..Default::default()
        },
    );
    API_CACHE.lock().get_mut(&known).unwrap().fetched_at =
        Instant::now() - API_CACHE_TTL - Duration::from_secs(60);
    let data = usage_api_for_profile(&known, |_| Ok((Some("t".into()), None)), rate_limited(None))
        .await
        .unwrap();
    assert_eq!(data.five_hour.unwrap().utilization, 37.0);
    let age = data
        .stale_age_secs
        .expect("a stale reading carries its age");
    assert!((360..400).contains(&age), "age {age}");
    for profile in [none, known] {
        API_CACHE.lock().remove(&profile);
        RATE_LIMITED_UNTIL.lock().remove(&profile);
    }
}

/// Catches usage state living only in memory: a restart inside the 5-minute
/// TTL (or inside a 429 cooldown) would poll the shared OAuth quota again.
#[tokio::test]
async fn usage_state_survives_a_restart_so_no_request_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage-state.json");
    let fresh = CredentialProfile::ConfigDir("/fixture/restart-fresh-2819".into());
    let cooling = CredentialProfile::ConfigDir("/fixture/restart-cooling-2819".into());
    let plan = PlanInfo {
        subscription_type: Some("team".into()),
        ..Default::default()
    };
    let mut data = UsageApiResponse {
        five_hour: Some(RateBucket {
            utilization: 55.0,
            resets_at: None,
        }),
        ..Default::default()
    };
    data.plan = Some(plan);
    cache_response(&fresh, &data);
    cache_response(&cooling, &data);
    API_CACHE.lock().get_mut(&cooling).unwrap().fetched_at =
        Instant::now() - API_CACHE_TTL - Duration::from_secs(30);
    record_usage_backoff(&cooling, Some(240));
    persist_profile(&path, &fresh);
    persist_profile(&path, &cooling);

    // Simulated restart: memory is empty, only the file remains.
    for profile in [&fresh, &cooling] {
        API_CACHE.lock().remove(profile);
        RATE_LIMITED_UNTIL.lock().remove(profile);
        hydrate_profile(&path, profile);
    }
    let restored = try_get_fresh_cache(&fresh).expect("inside the TTL after restart");
    assert_eq!(restored.five_hour.unwrap().utilization, 55.0);
    assert_eq!(
        restored.plan.unwrap().subscription_type.as_deref(),
        Some("team")
    );
    let until = *RATE_LIMITED_UNTIL
        .lock()
        .get(&cooling)
        .expect("cooldown restored");
    let remaining = until.saturating_duration_since(Instant::now()).as_secs();
    assert!((200..=240).contains(&remaining), "remaining {remaining}");
    assert!(try_get_fresh_cache(&cooling).is_none());
    let served = usage_api_for_profile(
        &cooling,
        |_| panic!("a cooldown restored from disk must not read credentials"),
        |_| async { panic!("a cooldown restored from disk must not fetch") },
    )
    .await
    .unwrap();
    assert!(served.stale_age_secs.unwrap() >= 300);
    // A cooldown that ended while the app was closed is not restored.
    RATE_LIMITED_UNTIL.lock().remove(&cooling);
    let mut state = read_usage_state(&path);
    state
        .get_mut(&profile_state_key(&cooling))
        .unwrap()
        .backoff_until_ms = Some(unix_now_ms() - 1);
    std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();
    API_CACHE.lock().remove(&cooling);
    hydrate_profile(&path, &cooling);
    assert!(!RATE_LIMITED_UNTIL.lock().contains_key(&cooling));
    for profile in [fresh, cooling] {
        API_CACHE.lock().remove(&profile);
        RATE_LIMITED_UNTIL.lock().remove(&profile);
    }
}

// Catches the usage ticker showing "offline" for a throttled endpoint, a missing
// login or an expired login. `describeUsageError` (src/features/agentUsage.ts)
// classifies these only by the backend error text: `Claude usage rate-limited,
// retry HH:MM`, the `No Claude OAuth token` hint, and `401`/`403` for an
// expired token. A Debug-formatted registry state matches none of them.
#[tokio::test]
async fn registry_usage_errors_keep_the_ticker_classification() {
    use crate::provider_quota::{
        BoxFuture, QuotaError, QuotaOwner, QuotaRegistry, QuotaSnapshot, SubscriptionKey,
    };
    struct Failing(fn() -> QuotaError);
    impl QuotaOwner for Failing {
        fn owner(&self) -> &'static str {
            "claude-code"
        }
        fn read<'a>(
            &'a self,
            _: &'a SubscriptionKey,
        ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
            let error = (self.0)();
            Box::pin(async move { Err(error) })
        }
    }
    async fn error_for(name: &str, error: fn() -> QuotaError) -> String {
        let registry = QuotaRegistry::default();
        registry.register_owner(Arc::new(Failing(error)));
        let key = SubscriptionKey::new("claude-code", "anthropic", format!("/fixture/{name}"));
        registry_usage(&registry, &key).await.unwrap_err()
    }

    // Adapter errors as produced by provider_quota::adapters (get_json, claude_credentials,
    // claude_with_credentials).
    let throttled = error_for("ticker-429-2988", || QuotaError::RateLimited {
        retry_after_secs: Some(3600),
    })
    .await;
    assert!(
        regex::Regex::new(r"^Claude usage rate-limited, retry \d{2}:\d{2}$")
            .unwrap()
            .is_match(&throttled),
        "{throttled}"
    );
    let missing = error_for("ticker-missing-2988", || {
        QuotaError::Unavailable("login not found".into())
    })
    .await;
    assert!(missing.contains("No Claude OAuth token"), "{missing}");
    for (name, error) in [
        (
            "ticker-expired-2988",
            (|| QuotaError::Auth("Claude login expired".into())) as fn() -> QuotaError,
        ),
        ("ticker-401-2988", || {
            QuotaError::Auth("HTTP 401 Unauthorized".into())
        }),
    ] {
        let expired = error_for(name, error).await;
        assert!(
            expired.contains("401") || expired.contains("403"),
            "{name}: {expired}"
        );
    }
}

// Catches the dashboard sending another HTTP read after the registry refreshed the same credential.
#[tokio::test]
async fn registry_and_dashboard_share_one_quota_request() {
    use crate::provider_quota::{
        BoxFuture, QuotaError, QuotaOwner, QuotaRegistry, QuotaSnapshot, QuotaSource, QuotaWindow,
        SubscriptionKey,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Owner(Arc<AtomicUsize>);
    impl QuotaOwner for Owner {
        fn owner(&self) -> &'static str {
            "claude-code"
        }
        fn read<'a>(
            &'a self,
            _: &'a SubscriptionKey,
        ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(QuotaSnapshot {
                    windows: vec![
                        QuotaWindow::from_duration(41.0, Some(18000), None, None).unwrap(),
                    ],
                    detail: Some(crate::provider_quota::QuotaDetail::Claude {
                        observed_at_ms: unix_now_ms(),
                        response: UsageApiResponse {
                            five_hour: Some(RateBucket {
                                utilization: 41.0,
                                resets_at: None,
                            }),
                            ..Default::default()
                        },
                    }),
                    plan: None,
                    credits: None,
                    account: None,
                    observed_at_ms: crate::provider_quota::Clock::now_ms(
                        &crate::provider_quota::SystemClock,
                    ),
                    source: QuotaSource::Active,
                })
            })
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = QuotaRegistry::default();
    registry.register_owner(Arc::new(Owner(calls.clone())));
    let key = SubscriptionKey::new("claude-code", "anthropic", "/fixture/shared-2988");
    registry.ensure(&key).await;
    let (data, stale) = registry_usage(&registry, &key).await.unwrap();
    assert!(!stale);
    assert_eq!(data.five_hour.unwrap().utilization, 41.0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "registry and dashboard issued duplicate requests"
    );
}
