//! Story092 batch 6: the wake signal.
//!
//! A host is not always reading the stream. It may be a phone with the app in
//! the background, or a window nobody has opened yet. What it needs then is not
//! the turn — it is being told that there is something worth coming back for,
//! and where.
//!
//! So the notice bus carries four things and nothing else: the connection is
//! usable, something you were waiting on finished, the agent is waiting on a
//! person, that question was answered. Everything a turn actually says stays on
//! the per-connection stream, which is what the rest of this suite proves; what
//! is proved here is that the chunks never reach the shared bus, because a bus
//! every subscriber in the app shares cannot carry one connection's chatter
//! without starving the rest of them.

use agent_client_protocol::schema::v1;
use tokio::sync::broadcast::Receiver;
use tuicommander_lib::acp::{AcpClientEvent, AcpNotice, AcpNoticeKind};

use crate::acp_support;

use acp_support::{Fixture, PATIENCE, authority, text, until};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";

fn selected(option: &str) -> v1::RequestPermissionOutcome {
    v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
        v1::PermissionOptionId::new(option),
    ))
}

/// The next notice, or a failure that says which one never came.
///
/// Bounded for the same reason every wait in this suite is: a notice that is
/// never published would otherwise hang the run instead of failing it.
async fn next_notice(notices: &mut Receiver<AcpNotice>) -> AcpNotice {
    tokio::time::timeout(PATIENCE, notices.recv())
        .await
        .expect("a notice was expected and none arrived")
        .expect("the notice bus outlives the connections on it")
}

fn named_session(notice: &AcpNotice) -> Option<&str> {
    notice.session_id.as_ref().map(|id| id.0.as_ref())
}

/// One connection's whole life, as the shared bus sees it.
///
/// Five notices for a turn that produced far more than five events, and each
/// one names the sequence it came from — which is the entire point: a host
/// woken by any of them resumes its stream from exactly there instead of
/// replaying a conversation it has already seen.
#[tokio::test]
async fn a_connection_wakes_a_host_on_ready_on_a_question_and_on_settling() {
    let fixture = Fixture::with("permission-turn");
    // Subscribed before the connection exists: the notice announcing it is the
    // one a subscriber that waited would have missed.
    let mut notices = fixture.manager.notices();
    let connection = fixture.connect().await;

    let ready = next_notice(&mut notices).await;
    assert_eq!(ready.kind, AcpNoticeKind::Ready);
    assert_eq!(ready.connection_id, connection.connection_id);
    assert_eq!(ready.generation, connection.generation);
    assert_eq!(
        ready.sequence, connection.latest_sequence,
        "the snapshot handed back by connect already covers the notice about it"
    );
    assert!(ready.session_id.is_none());
    assert!(ready.request_id.is_none());

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

    // Attaching a session lands it Idle, and an attachment reaching Idle is a
    // wake (a held turn released to Idle must reach a waiting host). Starting
    // a turn is not, so the next notice after that is the agent stopping to ask.
    let attached = next_notice(&mut notices).await;
    assert_eq!(attached.kind, AcpNoticeKind::Settled);
    assert_eq!(named_session(&attached), Some(SESSION));
    let asking = next_notice(&mut notices).await;
    assert_eq!(asking.kind, AcpNoticeKind::InteractionPending);
    assert_eq!(named_session(&asking), Some(SESSION));
    let request_id = asking
        .request_id
        .expect("a question a host has to answer names itself");

    fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the answer is accepted");

    // Sent even though this process is the one that answered: another client on
    // the same host may be showing that question, and it has to take it down.
    let answered = next_notice(&mut notices).await;
    assert_eq!(answered.kind, AcpNoticeKind::InteractionSettled);
    assert_eq!(answered.request_id, Some(request_id));
    assert_eq!(named_session(&answered), Some(SESSION));

    let settled = next_notice(&mut notices).await;
    assert_eq!(settled.kind, AcpNoticeKind::Settled);
    assert_eq!(
        named_session(&settled),
        Some(SESSION),
        "a turn settling names the session it belonged to"
    );

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    let updates = seen
        .iter()
        .filter(|event| matches!(event.event, AcpClientEvent::SessionUpdate { .. }))
        .count();
    assert!(
        updates >= 2,
        "this turn should have chattered on the stream: {seen:?}"
    );
    assert!(
        notices.try_recv().is_err(),
        "not one of those {updates} updates was allowed onto the shared bus"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .expect("disconnect");
    let closed = next_notice(&mut notices).await;
    assert_eq!(closed.kind, AcpNoticeKind::Settled);
    assert_eq!(closed.connection_id, connection.connection_id);
    assert!(
        closed.session_id.is_none(),
        "a connection settling belongs to no session, which is how it is told \
         apart from a turn settling"
    );
}
