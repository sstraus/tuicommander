//! Lifecycle failures must leave durable state untouched and revoke the one-use grant.
use super::{
    policy::Caller,
    policy_baseline_tests::{fixture, revision},
    policy_tests::answer,
    request::*,
};
use serde_json::json;

fn edit(rev: String) -> Request {
    Request::Set {
        target: Target::Machine,
        store: "app".into(),
        path: "/services/auth/username".into(),
        value: json!("verification-user"),
        expected_revision: rev,
    }
}

// Catches: a dialog holding the config lock, or a changed revision leaving a reusable approval.
#[tokio::test]
#[serial_test::serial]
async fn pending_dialog_holds_no_owner_lock_and_changed_revision_revokes_approval() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::native();
    let request = edit(revision(&service).await);
    let id = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let proposal = id.clone();
    let task = tokio::spawn(async move {
        crate::mcp_http::confirm_settings(&host, &Caller::native(), &proposal, None).await
    });
    let request_id = loop {
        if let crate::state::AppEvent::McpConfirm { request_id, .. } = events.recv().await.unwrap()
        {
            break request_id;
        }
    };
    // This real read + file commit completes while the human-response channel is still pending.
    let before = revision(&service).await;
    service
        .patch(
            Target::Machine,
            "app".into(),
            before,
            vec![Edit::Set {
                path: "/font_size".into(),
                value: json!(23),
            }],
        )
        .await
        .unwrap();
    let bytes = std::fs::read(dir.path().join("config.json")).unwrap();
    crate::mcp_http::resolve_mcp_confirm(&state, &request_id, true);
    assert_eq!(task.await.unwrap()["confirmed"], json!(true));
    let error = service
        .execute_as(request.clone(), &caller, Some(&id))
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    assert_eq!(
        service
            .execute_as(request, &caller, Some(&id))
            .await
            .unwrap_err()
            .code,
        "confirmation_denied"
    );
    assert_eq!(
        std::fs::read(dir.path().join("config.json")).unwrap(),
        bytes
    );
}

// Catches: a proposal's absolute 300s lifetime being extended by opening its dialog late.
#[tokio::test(start_paused = true)]
#[serial_test::serial]
async fn late_dialog_expires_at_proposal_deadline_and_cannot_write() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::native();
    let request = edit(revision(&service).await);
    let id = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    tokio::time::advance(std::time::Duration::from_secs(299)).await;
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let proposal = id.clone();
    let task = tokio::spawn(async move {
        crate::mcp_http::confirm_settings(&host, &Caller::native(), &proposal, None).await
    });
    loop {
        if matches!(
            events.recv().await.unwrap(),
            crate::state::AppEvent::McpConfirm { .. }
        ) {
            break;
        }
    }
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    assert_eq!(task.await.unwrap()["confirmed"], json!(false));
    assert!(state.confirm_responses.is_empty());
    assert!(
        service
            .execute_as(request, &caller, Some(&id))
            .await
            .is_err()
    );
    assert!(!dir.path().join("config.json").exists());
}

// Catches: a cancelled confirm handler leaving its proposal or UI question usable.
#[tokio::test]
#[serial_test::serial]
async fn cancelled_dialog_revokes_approval_and_dismisses_question() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::native();
    let request = edit(revision(&service).await);
    let id = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let proposal = id.clone();
    let task = tokio::spawn(async move {
        crate::mcp_http::confirm_settings(&host, &Caller::native(), &proposal, None).await
    });
    let request_id = loop {
        if let crate::state::AppEvent::McpConfirm { request_id, .. } = events.recv().await.unwrap()
        {
            break request_id;
        }
    };
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    crate::mcp_http::resolve_mcp_confirm(&state, &request_id, true);
    assert!(state.confirm_responses.is_empty());
    assert!(
        service
            .execute_as(request, &caller, Some(&id))
            .await
            .is_err()
    );
    assert!(!dir.path().join("config.json").exists());
}

// Catches: a valid approval applied to another connection despite unchanged content.
#[tokio::test]
#[serial_test::serial]
async fn approval_cannot_cross_connection_with_identical_content() {
    let (dir, _guard, state, service) = fixture();
    let caller = Caller::http("owner-one".into());
    let request = edit(revision(&service).await);
    let id = service
        .execute_as(request.clone(), &caller, None)
        .await
        .unwrap_err()
        .proposal_id
        .unwrap();
    assert_eq!(
        answer(state.clone(), caller, id.clone(), true).await["confirmed"],
        json!(true)
    );
    let other = Caller::http("owner-two".into());
    assert_eq!(
        service
            .execute_as(request, &other, Some(&id))
            .await
            .unwrap_err()
            .code,
        "confirmation_denied"
    );
    assert!(!dir.path().join("config.json").exists());
}
