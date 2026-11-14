//! A paused ACP turn remains resumable when ego settles its prompt with -32011.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpAttachmentState, AcpClientEvent, AcpHoldState, AcpNoticeKind, EgoHoldRequest,
};

use crate::acp_support;

use acp_support::{Fixture, authority, text, until};

const HOLD_REQUEST: &str = "01932d5e-0000-7000-8000-0000000000f1";

fn hold(session_id: &v1::SessionId) -> EgoHoldRequest {
    EgoHoldRequest {
        session_id: session_id.clone(),
        request_id: HOLD_REQUEST.parse().expect("fixed request id"),
    }
}

#[tokio::test]
async fn pause_error_keeps_resume_available_and_resume_allows_another_turn() {
    let fixture = Fixture::with("ego-pause-settlement");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id;
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();

    fixture
        .manager
        .prompt(
            connection.connection_id,
            session_id.clone(),
            vec![text("first")],
        )
        .await
        .expect("first prompt");
    let paused = fixture
        .manager
        .pause_turn(connection.connection_id, hold(&session_id))
        .await
        .expect("pause request");
    assert_eq!(paused.state, AcpHoldState::Pending);
    let pending = until(&mut stream, |event| {
        matches!(
            event,
            AcpClientEvent::AttachmentState {
                state: AcpAttachmentState::PausePending
            }
        )
    })
    .await;
    let settled = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnFailed { .. })
    })
    .await;
    assert_eq!(
        pending.last().unwrap().session_id.as_ref(),
        Some(&session_id)
    );
    assert_eq!(
        settled.last().unwrap().event,
        AcpClientEvent::TurnFailed {
            message: "session held at a boundary".to_string(),
            state: AcpAttachmentState::Paused
        },
        "the pause boundary leaves Resume available"
    );
    let attachment = &fixture
        .manager
        .snapshot(connection.connection_id)
        .unwrap()
        .attachments[0];
    assert_eq!(attachment.state, AcpAttachmentState::Paused);
    assert!(
        attachment.active_turn.is_none(),
        "the failed turn is finished"
    );

    let resumed = fixture
        .manager
        .resume_turn(connection.connection_id, hold(&session_id))
        .await
        .expect("resume held session");
    assert_eq!(resumed.state, AcpHoldState::Running);
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .state,
        AcpAttachmentState::Idle,
        "Resume is no longer offered after the hold releases"
    );
    fixture
        .manager
        .prompt(connection.connection_id, session_id, vec![text("second")])
        .await
        .expect("second prompt after resume");
    let finished = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert!(matches!(
        finished.last().unwrap().event,
        AcpClientEvent::TurnSettled { .. }
    ));
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn ordinary_prompt_error_without_hold_returns_to_idle_even_with_pause_wording() {
    let fixture = Fixture::with("ego-unheld-error");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let next_sequence = fixture
        .manager
        .snapshot(connection.connection_id)
        .unwrap()
        .latest_sequence
        + 1;
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, next_sequence)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id,
            vec![text("fail")],
        )
        .await
        .expect("prompt starts");
    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnFailed { .. })
    })
    .await;
    assert!(seen.iter().any(|event| matches!(
        &event.event,
        AcpClientEvent::TurnFailed { message, .. } if message == "session held at a boundary"
    )));
    assert_eq!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .state,
        AcpAttachmentState::Idle
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

// Catches: releasing a held turn to Idle sends no notice, so a workflow answer
// that waited for the conversation to be quiescent sleeps until the run deadline.
#[tokio::test]
async fn release_to_idle_wakes_hosts_once() {
    let fixture = Fixture::with("ego-pause-settlement");
    let mut notices = fixture.manager.notices();
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id;
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session_id.clone(),
            vec![text("first")],
        )
        .await
        .expect("first prompt");
    fixture
        .manager
        .pause_turn(connection.connection_id, hold(&session_id))
        .await
        .expect("pause request");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnFailed { .. })
    })
    .await;
    let held_at = fixture
        .manager
        .snapshot(connection.connection_id)
        .unwrap()
        .latest_sequence;
    while notices.try_recv().is_ok() {}

    fixture
        .manager
        .resume_turn(connection.connection_id, hold(&session_id))
        .await
        .expect("resume held session");

    let woke = tokio::time::timeout(acp_support::PATIENCE, notices.recv())
        .await
        .expect("release to Idle must wake hosts")
        .expect("notice bus open");
    assert_eq!(woke.kind, AcpNoticeKind::Settled);
    assert_eq!(woke.session_id.as_ref(), Some(&session_id));
    assert!(woke.sequence > held_at);
    assert!(
        notices.try_recv().is_err(),
        "one release is one wake, not a stream of them"
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
