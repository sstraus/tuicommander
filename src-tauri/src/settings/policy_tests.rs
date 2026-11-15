//! Approval contracts use the existing all-client event and human-response channel.
use super::{
    policy::Caller,
    policy_baseline_tests::{fixture, revision},
    request::*,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn edit(rev: String, username: &str) -> Request {
    Request::Set {
        target: Target::Machine,
        store: "app".into(),
        path: "/services/auth/username".into(),
        value: json!(username),
        expected_revision: rev,
    }
}
pub(super) async fn answer(
    state: Arc<crate::AppState>,
    caller: Caller,
    id: String,
    approved: bool,
) -> Value {
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let mut task =
        tokio::spawn(
            async move { crate::mcp_http::confirm_settings(&host, &caller, &id, None).await },
        );
    loop {
        let event = tokio::select! {
            event = events.recv() => event.unwrap(),
            _ = &mut task => panic!("confirmation ended before showing the dialog"),
        };
        if let crate::state::AppEvent::McpConfirm { request_id, .. } = event {
            crate::mcp_http::resolve_mcp_confirm(&state, &request_id, approved);
            break;
        }
    }
    task.await.unwrap()
}

// Catches: an authenticated remote MCP connection still refused a write that loopback may commit.
#[tokio::test]
#[serial_test::serial]
async fn mcp_patch_commits_like_loopback() {
    let (dir, _guard, _state, service) = fixture();
    let caller = Caller::mcp("remote-connection".into(), true);
    let req = Request::Set {
        target: Target::Machine,
        store: "app".into(),
        path: "/font_size".into(),
        value: json!(21),
        expected_revision: revision(&service).await,
    };
    let reply = service.execute_as(req, &caller, None).await.unwrap();
    assert!(matches!(
        reply,
        Reply::Commit(CommitReply {
            persisted: true,
            ..
        })
    ));
    let disk: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(disk["font_size"], json!(21));
}

// Catches: secret input entering error, confirm broadcast or bounded audit text.
#[tokio::test]
#[serial_test::serial]
async fn secret_never_in_errors_events() {
    let (dir, _guard, state, service) = fixture();
    let secret = uuid::Uuid::new_v4().to_string();
    state.config.write().services.auth.session_token = secret.clone();
    let rev = revision(&service).await;
    let invalid = service
        .patch(
            Target::Machine,
            "app".into(),
            rev.clone(),
            vec![Edit::Set {
                path: "/font_size".into(),
                value: json!(secret),
            }],
        )
        .await
        .unwrap_err();
    assert!(!serde_json::to_string(&invalid).unwrap().contains(&secret));
    let error = service
        .execute_as(edit(rev, "verification-user"), &Caller::native(), None)
        .await
        .unwrap_err();
    assert!(!serde_json::to_string(&error).unwrap().contains(&secret));
    let id = error.proposal_id.unwrap();
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let task = tokio::spawn(async move {
        crate::mcp_http::confirm_settings(&host, &Caller::native(), &id, None).await
    });
    loop {
        let event = events.recv().await.unwrap();
        assert!(!serde_json::to_string(&event).unwrap().contains(&secret));
        if let crate::state::AppEvent::McpConfirm {
            request_id,
            message,
            ..
        } = event
        {
            assert!(message.contains("app.services.auth.username"));
            assert!(message.contains("verification-user"));
            eprintln!(
                "POLICY_DIALOG_PREVIEW={}",
                serde_json::json!({"title":"Approve settings change", "message":message})
            );
            crate::mcp_http::resolve_mcp_confirm(&state, &request_id, false);
            break;
        }
    }
    assert_eq!(task.await.unwrap()["confirmed"], json!(false));
    assert!(
        !serde_json::to_string(&state.settings_policy.audit())
            .unwrap()
            .contains(&secret)
    );
    assert!(!dir.path().join("config.json").exists());
}

// Catches: one human grant reused, substituted onto other content or another connection.
#[tokio::test]
#[serial_test::serial]
async fn approval_not_replayable() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::native();
    let request = edit(revision(&service).await, "verification-user");
    let id = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    assert_eq!(
        answer(state.clone(), caller.clone(), id.clone(), true).await["confirmed"],
        json!(true)
    );
    let result = service
        .execute_as(request.clone(), &caller, Some(&id))
        .await
        .unwrap();
    assert!(matches!(
        result,
        Reply::Commit(CommitReply {
            persisted: true,
            ..
        })
    ));
    let bytes = std::fs::read(dir.path().join("config.json")).unwrap();
    assert!(
        service
            .execute_as(request, &caller, Some(&id))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(dir.path().join("config.json")).unwrap(),
        bytes
    );
    // A valid grant for one revision cannot authorize content substitution or another caller.
    for substituted_caller in [caller.clone(), Caller::http("another-connection".into())] {
        let request = edit(revision(&service).await, "next-user");
        let id = service
            .execute_as(request.clone(), &caller, None)
            .await
            .unwrap_err()
            .proposal_id
            .unwrap();
        assert_eq!(
            answer(state.clone(), caller.clone(), id.clone(), true).await["confirmed"],
            json!(true)
        );
        let modified = edit(revision(&service).await, "substituted-user");
        assert!(
            service
                .execute_as(modified, &substituted_caller, Some(&id))
                .await
                .is_err()
        );
        assert!(
            service
                .execute_as(request, &caller, Some(&id))
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(dir.path().join("config.json")).unwrap(),
            bytes
        );
    }
}

// Catches: refusal, exact 300-second expiry or connection teardown allowing a later write.
#[tokio::test(start_paused = true)]
#[serial_test::serial]
async fn expired_confirm_no_write() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::mcp("expiry-connection".into(), true);
    for cause in ["refusal", "expiry", "disconnect"] {
        let request = edit(revision(&service).await, "verification-user");
        let id = service
            .execute_as(request.clone(), &caller, None)
            .await
            .unwrap_err()
            .proposal_id
            .unwrap();
        match cause {
            "refusal" => {
                assert_eq!(
                    answer(state.clone(), caller.clone(), id.clone(), false).await["confirmed"],
                    json!(false)
                );
            }
            "expiry" => {
                tokio::time::advance(std::time::Duration::from_secs(300)).await;
            }
            _ => {
                state.settings_policy.disconnect_mcp("expiry-connection");
            }
        }
        assert!(
            service
                .execute_as(request, &caller, Some(&id))
                .await
                .is_err()
        );
        assert!(!dir.path().join("config.json").exists());
    }
}
