//! Story 2637: what an AI Chat is doing, derived once in Rust from ACP.
//!
//! Every host renders the same `activity` for a chat: the desktop panel, a
//! browser and a phone. These tests prove that the meaning comes from the
//! connection's own facts — the turn, the open seats, the tool calls and ego's
//! negotiated child frames — and that every way a wait can end takes it off
//! the snapshot and the stream together, so a host that reconnects from
//! either cannot bring an answered question back.

use agent_client_protocol::schema::v1;
use serde_json::json;
use tuicommander_lib::acp::{
    AcpChatActivity, AcpChatActivityState, AcpClientEvent, AcpConnectionId, AcpEventEnvelope,
    AcpHumanWaitKind, capability_snapshot,
};

use crate::acp_support;

use acp_support::{Fixture, authority, text, until};

const CHAT_A: &str = "01932d5e-0000-7000-8000-0000000000aa";
const CHAT_B: &str = "01932d5e-0000-7000-8000-0000000000bb";

fn selected(option: &str) -> v1::RequestPermissionOutcome {
    v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
        v1::PermissionOptionId::new(option),
    ))
}

/// The activity a stream event carries for one session, if it carries one.
fn activity_of<'a>(event: &'a AcpEventEnvelope, session: &str) -> Option<&'a AcpChatActivity> {
    match &event.event {
        AcpClientEvent::ActivityChanged { activity }
            if event.session_id.as_ref().map(|id| id.0.as_ref()) == Some(session) =>
        {
            Some(activity)
        }
        _ => None,
    }
}

fn snapshot_activity(
    fixture: &Fixture,
    connection: AcpConnectionId,
    session: &str,
) -> AcpChatActivity {
    fixture
        .manager
        .snapshot(connection)
        .unwrap()
        .attachments
        .into_iter()
        .find(|attachment| attachment.session_id.0.as_ref() == session)
        .unwrap_or_else(|| panic!("no attachment for {session}"))
        .activity
}

/// Everything the journal holds, as a host that reconnects from 0 reads it.
fn replayed(fixture: &Fixture, connection: AcpConnectionId) -> Vec<AcpEventEnvelope> {
    fixture
        .manager
        .page(connection, 0, None, None)
        .expect("page")
        .events
}

/// The last activity the stream says a session has.
fn last_streamed(events: &[AcpEventEnvelope], session: &str) -> AcpChatActivity {
    events
        .iter()
        .filter_map(|event| activity_of(event, session))
        .next_back()
        .cloned()
        .unwrap_or_else(|| panic!("no activity was streamed for {session}"))
}

/// Prompt chat A, wait until its question is open, and hand back the stream.
async fn awaiting(
    fixture: &Fixture,
) -> (
    AcpConnectionId,
    v1::SessionId,
    tuicommander_lib::acp::AcpEventStream,
    tuicommander_lib::acp::AcpHostRequestId,
) {
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");
    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { activity }
            if activity.state == AcpChatActivityState::Awaiting)
    })
    .await;
    let waits = &last_streamed(&seen, CHAT_A).human_waits;
    assert_eq!(waits.len(), 1, "{waits:?}");
    assert_eq!(waits[0].kind, AcpHumanWaitKind::Permission);
    (
        connection.connection_id,
        session.session_id,
        stream,
        waits[0].request_id,
    )
}

/// After a wait ends, the snapshot and the replayed stream agree that it is
/// gone, and the snapshot's revision is past the one that showed it.
fn assert_wait_retracted(fixture: &Fixture, connection: AcpConnectionId, awaiting_revision: u64) {
    let held = snapshot_activity(fixture, connection, CHAT_A);
    assert!(held.human_waits.is_empty(), "{held:?}");
    assert_ne!(held.state, AcpChatActivityState::Awaiting);
    assert!(held.revision > awaiting_revision, "{held:?}");
    assert_eq!(last_streamed(&replayed(fixture, connection), CHAT_A), held);
}

// catches: a child frame for chat A marking every chat on the shared ego
// process busy, or a second prompt into A's occupied turn reaching the wire
// because A's live child was taken for "the turn is still busy elsewhere".
#[tokio::test]
async fn acp_child_in_chat_a_does_not_mark_chat_b_busy() {
    let fixture = Fixture::with("child-activity-two-chats");
    let connection = fixture.connect().await;
    assert_eq!(
        connection
            .capabilities
            .as_ref()
            .and_then(|caps| caps.ego_child_activity_version),
        Some(1)
    );
    let id = connection.connection_id;
    let chat_a = fixture
        .manager
        .new_session(id, authority(fixture.root()))
        .await
        .expect("chat A");
    let chat_b = fixture
        .manager
        .new_session(id, authority(fixture.root()))
        .await
        .expect("chat B");
    assert_eq!(chat_a.session_id.0.as_ref(), CHAT_A);
    assert_eq!(chat_b.session_id.0.as_ref(), CHAT_B);
    let mut stream = fixture.manager.subscribe(id, 0).expect("subscribe");

    fixture
        .manager
        .prompt(id, chat_a.session_id.clone(), vec![text("run a child")])
        .await
        .expect("prompt A");
    let mut seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { activity } if !activity.children.is_empty())
    })
    .await;

    let a = snapshot_activity(&fixture, id, CHAT_A);
    assert_eq!(a.state, AcpChatActivityState::Busy);
    assert_eq!(a.children.len(), 1);
    assert_eq!(a.children[0].child["kind"], "foreground");
    assert_eq!(a.tools.len(), 1, "the recorded tool call is still running");
    let b = snapshot_activity(&fixture, id, CHAT_B);
    assert_eq!(
        b,
        AcpChatActivity::default(),
        "chat B has not done anything"
    );

    // The occupied turn queues the second prompt; only chat B's own prompt
    // reaches the wire next, which the scenario checks by session id.
    fixture
        .manager
        .prompt(id, chat_a.session_id.clone(), vec![text("and then")])
        .await
        .expect("prompt A again");
    let attachment_a = fixture
        .manager
        .snapshot(id)
        .unwrap()
        .attachments
        .into_iter()
        .find(|attachment| attachment.session_id == chat_a.session_id)
        .unwrap();
    assert_eq!(attachment_a.queued_prompts.len(), 1);
    fixture
        .manager
        .prompt(id, chat_b.session_id.clone(), vec![text("my own turn")])
        .await
        .expect("prompt B");

    let mut settled_a = 0;
    while settled_a < 2 {
        let more = until(&mut stream, |event| {
            matches!(event, AcpClientEvent::TurnSettled { .. })
        })
        .await;
        settled_a += usize::from(
            more.last().and_then(|event| event.session_id.as_ref()) == Some(&chat_a.session_id),
        );
        seen.extend(more);
    }
    let more = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { activity }
            if activity.state == AcpChatActivityState::Idle)
    })
    .await;
    seen.extend(more);

    for activity in seen.iter().filter_map(|event| activity_of(event, CHAT_B)) {
        assert!(
            activity.children.is_empty() && activity.tools.is_empty(),
            "{activity:?}"
        );
    }
    let a = snapshot_activity(&fixture, id, CHAT_A);
    assert_eq!(a.state, AcpChatActivityState::Idle);
    assert!(a.children.is_empty() && a.tools.is_empty() && !a.interrupted);
    assert_eq!(
        snapshot_activity(&fixture, id, CHAT_B).state,
        AcpChatActivityState::Idle
    );

    fixture.manager.disconnect(id).await.unwrap();
}

// catches: an answered question surviving on the snapshot or as the last
// streamed activity, so a host that reconnects from either shows it again.
#[tokio::test]
async fn acp_settled_question_does_not_survive_reconnect() {
    let fixture = Fixture::with("permission-turn");
    let (connection, _, mut stream, request_id) = awaiting(&fixture).await;
    let awaiting_revision = snapshot_activity(&fixture, connection, CHAT_A).revision;

    fixture
        .manager
        .respond_permission(connection, request_id, selected("allow"))
        .await
        .expect("answer");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert_wait_retracted(&fixture, connection, awaiting_revision);
    assert_eq!(
        snapshot_activity(&fixture, connection, CHAT_A).state,
        AcpChatActivityState::Idle
    );

    fixture.manager.disconnect(connection).await.unwrap();
}

// catches: only an "allow" answer retracting the wait, so a rejection leaves
// the chat asking.
#[tokio::test]
async fn a_rejected_permission_retracts_the_wait() {
    let fixture = Fixture::with("permission-rejected");
    let (connection, _, mut stream, request_id) = awaiting(&fixture).await;
    let awaiting_revision = snapshot_activity(&fixture, connection, CHAT_A).revision;

    fixture
        .manager
        .respond_permission(connection, request_id, selected("reject"))
        .await
        .expect("reject");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert_wait_retracted(&fixture, connection, awaiting_revision);

    fixture.manager.disconnect(connection).await.unwrap();
}

// catches: the stop button sweeping the seats without the activity noticing,
// or the cancelled turn not reading as interrupted afterwards.
#[tokio::test]
async fn stopping_a_turn_retracts_every_wait_and_marks_it_interrupted() {
    let fixture = Fixture::with("permission-cancel");
    let connection = fixture.connect().await;
    let id = connection.connection_id;
    let session = fixture
        .manager
        .new_session(id, authority(fixture.root()))
        .await
        .expect("session/new");
    let mut stream = fixture.manager.subscribe(id, 0).expect("subscribe");
    fixture
        .manager
        .prompt(id, session.session_id.clone(), vec![text("hello")])
        .await
        .expect("prompt");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { activity } if activity.human_waits.len() == 2)
    })
    .await;
    let asked = snapshot_activity(&fixture, id, CHAT_A);
    assert_eq!(
        asked
            .human_waits
            .iter()
            .map(|wait| wait.kind)
            .collect::<Vec<_>>(),
        [AcpHumanWaitKind::Permission, AcpHumanWaitKind::Question]
    );

    fixture
        .manager
        .cancel(id, session.session_id.clone())
        .await
        .expect("cancel");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert_wait_retracted(&fixture, id, asked.revision);
    let after = snapshot_activity(&fixture, id, CHAT_A);
    assert_eq!(after.state, AcpChatActivityState::Idle);
    assert!(after.interrupted);

    fixture.manager.disconnect(id).await.unwrap();
}

// catches: an agent that exits while a person is asked leaving the chat
// "awaiting" forever, since no settlement frame will ever arrive.
#[tokio::test]
async fn an_agent_exit_retracts_the_wait() {
    let fixture = Fixture::with("permission-then-exit");
    let (connection, _, mut stream, _) = awaiting(&fixture).await;
    let awaiting_revision = snapshot_activity(&fixture, connection, CHAT_A).revision;

    std::fs::write(fixture.root().join("exit-now"), b"").expect("release the agent");
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { activity }
            if activity.state == AcpChatActivityState::Idle)
    })
    .await;
    assert_wait_retracted(&fixture, connection, awaiting_revision);
    let after = snapshot_activity(&fixture, connection, CHAT_A);
    assert!(after.interrupted, "the running turn never finished");
    assert!(
        fixture
            .manager
            .snapshot(connection)
            .unwrap()
            .settlement
            .is_some()
    );
}

// catches: child frames read from an agent that never advertised them, or
// from one advertising a version or carrier this client was not written for.
#[test]
fn child_activity_is_negotiated_only_on_the_exact_advertisement() {
    let recorded: serde_json::Value = serde_json::from_str(
        include_str!("../fixtures/acp/ego-child-activity-native-0.4.1.jsonl")
            .lines()
            .nth(1)
            .unwrap(),
    )
    .unwrap();
    let mut result = recorded["message"]["result"].clone();
    let parse = |result: &serde_json::Value| {
        capability_snapshot(&serde_json::from_value(result.clone()).unwrap()).unwrap()
    };
    assert_eq!(parse(&result).ego_child_activity_version, Some(1));

    let old: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/acp/ego-initialize.json")).unwrap();
    assert_eq!(
        parse(&old).ego_child_activity_version,
        None,
        "not advertised"
    );

    result["agentCapabilities"]["_meta"]["ego"]["childActivity"]["version"] = json!(2);
    assert_eq!(
        parse(&result).ego_child_activity_version,
        None,
        "other version"
    );
    result["agentCapabilities"]["_meta"]["ego"]["childActivity"] =
        json!({"version": 1, "update": "agent_message_chunk", "field": "_meta.ego.childActivity"});
    assert_eq!(
        parse(&result).ego_child_activity_version,
        None,
        "other carrier"
    );
}

/// A resumed chat whose durable child is still running reads busy.
///
/// Catches: the roster snapshot ego sends for a `session/load` reaches the
/// journal while the session is still attaching, and the activity tracker is
/// fed only for recorded attachments, so the resumed chat reads idle while
/// its background child runs.
#[tokio::test]
async fn a_durable_child_in_the_load_snapshot_marks_the_resumed_chat_busy() {
    let fixture = Fixture::with("child-activity-durable-on-load");
    let connection = fixture.connect().await;

    let attachment = fixture
        .manager
        .attach(
            connection.connection_id,
            tuicommander_lib::acp::AcpAttachKind::Load,
            v1::SessionId::new(CHAT_A),
            authority(fixture.root()),
        )
        .await
        .expect("session/load");
    assert_eq!(attachment.session_id, v1::SessionId::new(CHAT_A));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let activity = &snapshot
        .attachments
        .iter()
        .find(|attachment| attachment.session_id == v1::SessionId::new(CHAT_A))
        .expect("chat A attached")
        .activity;
    assert_eq!(
        activity.children.len(),
        1,
        "the durable child of the load snapshot is missing: {activity:?}"
    );
    assert_eq!(activity.state, AcpChatActivityState::Busy, "{activity:?}");

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
