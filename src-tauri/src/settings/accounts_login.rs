//! Opaque, bounded device-login flows: the GitHub device credential never leaves the backend.
use super::ID;
use crate::{
    github_auth::{DeviceCodeResponse, PollResult},
    settings::request::{Error, Failure},
    state::AppState,
};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Logins {
    flows: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Flow>>>>,
}

struct Flow {
    code: String,
    epoch: u64,
    additional: bool,
    expires: Instant,
    next_poll: Instant,
    interval: Duration,
    status: &'static str,
}

impl Logins {
    pub(super) async fn begin(
        &self,
        state: &Arc<AppState>,
        additional: bool,
    ) -> Result<Value, Failure> {
        {
            let now = Instant::now();
            let mut flows = self.flows.lock();
            flows.retain(|_, flow| flow.try_lock().map_or(true, |flow| flow.expires > now));
            if flows.len() >= 16 {
                return Err(Error::new(ID, "out_of_range"));
            }
        }
        let epoch = state
            .github
            .login_epoch
            .load(std::sync::atomic::Ordering::SeqCst);
        let response = crate::github_auth::start_device_flow(&state.http_client)
            .await
            .map_err(|_| Error::new(ID, "unavailable"))?;
        self.insert(response, additional, epoch)
    }

    fn insert(
        &self,
        response: DeviceCodeResponse,
        additional: bool,
        epoch: u64,
    ) -> Result<Value, Failure> {
        // Bounds keep Instant arithmetic finite; device_code is never returned.
        if response.expires_in == 0
            || response.expires_in > 3600
            || response.interval == 0
            || response.interval > 300
        {
            return Err(Error::new(ID, "unavailable"));
        }
        let now = Instant::now();
        let mut flows = self.flows.lock();
        flows.retain(|_, flow| flow.try_lock().map_or(true, |flow| flow.expires > now));
        if flows.len() >= 16 {
            return Err(Error::new(ID, "out_of_range"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let interval = Duration::from_secs(response.interval);
        flows.insert(
            id.clone(),
            Arc::new(tokio::sync::Mutex::new(Flow {
                code: response.device_code,
                epoch,
                additional,
                expires: now + Duration::from_secs(response.expires_in),
                next_poll: now + interval,
                interval,
                status: "pending",
            })),
        );
        Ok(
            json!({"flowId":id,"status":"pending","verificationUri":response.verification_uri,"userCode":response.user_code,"expiresIn":response.expires_in,"interval":response.interval}),
        )
    }

    fn flow(&self, id: &str) -> Result<Arc<tokio::sync::Mutex<Flow>>, Failure> {
        self.flows
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::new(ID, "not_found"))
    }

    pub(super) fn status(&self, id: &str) -> Result<Value, Failure> {
        let flow = self.flow(id)?;
        let Ok(mut flow) = flow.try_lock() else {
            return Ok(json!({"status":"busy"}));
        };
        if flow.expires <= Instant::now() && flow.status == "pending" {
            flow.status = "expired";
            flow.code.clear();
        }
        Ok(json!({"status":flow.status,"interval":flow.interval.as_secs()}))
    }

    pub(super) async fn complete(&self, state: &Arc<AppState>, id: &str) -> Result<Value, Failure> {
        let flow = self.flow(id)?;
        let Ok(mut flow) = flow.try_lock_owned() else {
            return Ok(json!({"status":"busy"}));
        };
        let now = Instant::now();
        if flow.epoch
            != state
                .github
                .login_epoch
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            flow.status = "expired";
            flow.code.clear();
        }
        if flow.expires <= now && flow.status == "pending" {
            flow.status = "expired";
            flow.code.clear();
        }
        if flow.status != "pending" {
            return Ok(json!({"status":flow.status,"persisted":false}));
        }
        if now < flow.next_poll {
            return Ok(json!({"status":"pending","interval":flow.interval.as_secs()}));
        }
        // Consume one poll interval even on a provider failure. No token/result body is serialized.
        flow.next_poll = now + flow.interval;
        let result = if flow.additional {
            crate::github_auth::github_poll_add_account_impl_at_epoch(
                state,
                flow.code.clone(),
                flow.epoch,
            )
            .await
        } else {
            crate::github_auth::github_poll_login_impl_at_epoch(
                state,
                flow.code.clone(),
                flow.epoch,
            )
            .await
        }
        .map_err(|_| Error::new(ID, "unavailable"))?;
        match result {
            PollResult::Pending => {}
            PollResult::SlowDown => {
                flow.interval += Duration::from_secs(5);
                flow.next_poll = Instant::now() + flow.interval;
            }
            PollResult::Expired => flow.status = "expired",
            PollResult::AccessDenied => flow.status = "access_denied",
            PollResult::Success { .. } => flow.status = "success",
        }
        if flow.status != "pending" {
            flow.code.clear();
        }
        Ok(
            json!({"status":flow.status,"interval":flow.interval.as_secs(),"persisted":flow.status == "success"}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: an unknown/caller-selected device code being accepted as a host-issued flow.
    #[tokio::test]
    async fn unknown_flow_is_rejected_without_network_or_credential_access() {
        let logins = Logins::default();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        assert_eq!(
            logins.status("device-secret-canary").unwrap_err().code,
            "not_found"
        );
        let error = logins
            .complete(&state, "device-secret-canary")
            .await
            .unwrap_err();
        assert_eq!(error.code, "not_found");
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("device-secret-canary")
        );
    }

    // Catches: a flow issued before disconnect can still complete after logout.
    #[tokio::test]
    async fn device_login_started_before_disconnect_cannot_complete() {
        let logins = Logins::default();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let now = Instant::now();
        logins.flows.lock().insert(
            "before".into(),
            Arc::new(tokio::sync::Mutex::new(Flow {
                code: "private-device-canary".into(),
                epoch: 0,
                additional: false,
                expires: now + Duration::from_secs(60),
                next_poll: now + Duration::from_secs(60),
                interval: Duration::from_secs(5),
                status: "pending",
            })),
        );
        crate::github_auth::github_disconnect_impl(&state)
            .await
            .unwrap();
        let reply = logins.complete(&state, "before").await.unwrap();
        assert_eq!(reply["status"], "expired");
        assert_eq!(reply["persisted"], false);
    }

    // Catches: an expired device authorization being polled/persisted as a new login,
    // or terminal retries retaining the private device credential.
    #[tokio::test]
    async fn provider_authorization_deadline_cannot_authenticate_or_persist() {
        use super::super::{GithubAction, GithubSettings};
        let (_dir, _guard, state) = crate::settings::accounts_tests::isolated();
        let owner = GithubSettings::new(state.clone());
        let before = owner.read().await.unwrap();
        let token = state.github.token.read().clone();
        let mut events = state.event_bus.subscribe();
        for status_first in [false, true] {
            let id = if status_first {
                "status-first"
            } else {
                "complete-first"
            };
            let now = Instant::now();
            // TUIC-owned deadline state, not a fabricated provider HTTP response.
            // Both polling and authorization have reached their deadline: expiry must win.
            owner.login.flows.lock().insert(
                id.into(),
                Arc::new(tokio::sync::Mutex::new(Flow {
                    code: "expired-device-private-canary".into(),
                    epoch: state
                        .github
                        .login_epoch
                        .load(std::sync::atomic::Ordering::SeqCst),
                    additional: false,
                    expires: now,
                    next_poll: now,
                    interval: Duration::from_secs(5),
                    status: "pending",
                })),
            );
            if status_first {
                let status = owner
                    .perform(GithubAction::CloudLoginStatus { flow_id: id.into() })
                    .await
                    .unwrap();
                assert_eq!(status["status"], "expired");
            }
            for _ in 0..2 {
                let reply = owner
                    .perform(GithubAction::CompleteCloudLogin { flow_id: id.into() })
                    .await
                    .unwrap();
                assert_eq!(reply["status"], "expired", "{reply}");
                assert_eq!(reply["persisted"], false);
                assert!(!reply.to_string().contains("expired-device-private-canary"));
            }
            assert_eq!(owner.login.flow(id).unwrap().lock().await.code, "");
        }
        assert_eq!(owner.read().await.unwrap(), before);
        assert_eq!(*state.github.token.read(), token);
        assert!(events.try_recv().is_err());
    }

    // Catches: polling expired or completed flows again, or ignoring the owner's poll interval.
    #[tokio::test]
    async fn terminal_flows_and_poll_interval_do_not_exchange_device_credentials() {
        let logins = Logins::default();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for (id, status, expired, expected) in [
            ("fresh", "pending", false, "pending"),
            ("old", "pending", true, "expired"),
            ("done", "success", false, "success"),
            ("refused", "access_denied", false, "access_denied"),
        ] {
            let now = Instant::now();
            // Internal TUIC flow state, not a fabricated GitHub response fixture.
            logins.flows.lock().insert(
                id.into(),
                Arc::new(tokio::sync::Mutex::new(Flow {
                    code: "private-device-canary".into(),
                    epoch: 0,
                    additional: false,
                    expires: if expired {
                        now
                    } else {
                        now + Duration::from_secs(60)
                    },
                    next_poll: now + Duration::from_secs(60),
                    interval: Duration::from_secs(5),
                    status,
                })),
            );
            let result = logins.complete(&state, id).await.unwrap();
            assert_eq!(result["status"], expected);
            assert!(!result.to_string().contains("private-device-canary"));
            assert_eq!(logins.status(id).unwrap()["status"], expected);
        }
    }
}
