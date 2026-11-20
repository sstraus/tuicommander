use super::*;
use crate::workflows::neutral_api::model::NeutralAction;
use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::net::SocketAddr;
use tower::ServiceExt;

async fn post(f: &Fixture, action: &NeutralAction) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!(
            "/workflows/neutral/action?path={}",
            f.context.workspace_path().display()
        ))
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("cookie", "tui-session=operator-fixture-owner")
        .body(Body::from(serde_json::to_string(action).unwrap()))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 1234))));
    let response = crate::mcp_http::build_router(f.state.clone(), true, true)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

// Catches: native binding and typed operator checks escaping the refusal map
// as HTTP 500, or a refused answer committing a decision or advancing the run.
#[tokio::test]
async fn operator_http_refusals_keep_text_and_do_not_change_run() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = fixture();
    super::super::runtime::native_public_support::attach(&f);
    *f.state.session_token.write() = "operator-fixture-owner".into();
    let subject = request(&f);
    let run = start(&f, &subject, false);
    let waiting = f.dispatch(&run).await;
    let key = waiting.effects.values().next().unwrap().key.clone();
    let operator = NeutralAction::OperatorAnswer {
        run_id: waiting.run.id.clone(),
        command_id: "operator-http-answer".into(),
        expected_sequence: waiting.sequence,
        key: key.clone(),
        revision: 1,
        answer: OperatorAnswer {
            request: subject.clone(),
            accept: true,
            reason: "Exact human decision".into(),
        },
    };
    let mut missing_operator = operator.clone();
    if let NeutralAction::OperatorAnswer { run_id, .. } = &mut missing_operator {
        *run_id = "missing".into();
    }
    let missing_effect = NeutralAction::Answer {
        run_id: "missing".into(),
        command_id: "missing-effect-answer".into(),
        expected_sequence: waiting.sequence,
        key,
        revision: 1,
        approved: true,
    };
    let mut stale_sequence = operator.clone();
    if let NeutralAction::OperatorAnswer {
        expected_sequence, ..
    } = &mut stale_sequence
    {
        *expected_sequence -= 1;
    }
    let mut invalid_reason = operator.clone();
    if let NeutralAction::OperatorAnswer { answer, .. } = &mut invalid_reason {
        answer.reason.clear();
    }
    let mut stale_revision = operator.clone();
    if let NeutralAction::OperatorAnswer { revision, .. } = &mut stale_revision {
        *revision += 1;
    }
    for (name, action, status, text) in [
        (
            "unknown effect answer run",
            missing_effect,
            StatusCode::NOT_FOUND,
            "native trusted caller/run binding unavailable",
        ),
        (
            "unknown operator answer run",
            missing_operator,
            StatusCode::NOT_FOUND,
            "native trusted caller/run binding unavailable",
        ),
        (
            "stale operator sequence",
            stale_sequence,
            StatusCode::CONFLICT,
            "operator answer differs from exact current wait/sequence/reason",
        ),
        (
            "invalid operator reason",
            invalid_reason,
            StatusCode::CONFLICT,
            "operator answer differs from exact current wait/sequence/reason",
        ),
        (
            "stale operator revision",
            stale_revision,
            StatusCode::CONFLICT,
            "neutral approval is expired, stale or already answered",
        ),
    ] {
        let (actual, body) = post(&f, &action).await;
        assert_eq!(actual, status, "{name}: {body}");
        assert_eq!(body["error"], text, "{name}");
        let retained = f.owner.store.neutral_execution(&waiting.run.id).unwrap();
        assert_eq!(retained.sequence, waiting.sequence, "{name}");
        assert!(retained.effects.values().next().unwrap().answer.is_none());
    }
    // Commit through the real owner without the HTTP success path's actor wake:
    // the assertion below isolates the rejected replay, not legitimate dispatch
    // that a previous successful HTTP request may still be advancing.
    let accepted = answer(&f, &waiting, &subject, true);
    let answered = f
        .owner
        .neutral_effect_command(&waiting.run.id, accepted)
        .unwrap();
    let (status, body) = post(&f, &operator).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "operator answer differs from exact current wait/sequence/reason"
    );
    assert_eq!(
        f.owner
            .store
            .neutral_execution(&waiting.run.id)
            .unwrap()
            .sequence,
        answered.sequence
    );
}
