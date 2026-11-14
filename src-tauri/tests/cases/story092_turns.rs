//! Story092 batch 4: turns, their updates, and how they end.
//!
//! A turn is the one thing on this connection that outlives the call that
//! started it. It takes as long as a model takes, it produces output the whole
//! time, and it can be asked to stop halfway. Everything here follows from
//! that: the caller is handed a turn id rather than an outcome, the outcome and
//! the output travel on the event stream where more than one reader can see
//! them, and the actor stays free to act while the turn is running.
//!
//! Ego owns what the turn actually did. What is proved here is that this client
//! reports it in the order it happened, does not invent an ending, and does not
//! quietly send content the agent said it cannot read.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpAttachKind, AcpAttachmentState, AcpChatActivityState, AcpClientErrorCode, AcpClientEvent,
    AcpDetachKind, AcpOperation, AcpTurnState,
};

use crate::acp_support;

use acp_support::{Fixture, authority, chunk, text, until, until_settled};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";

#[tokio::test]
async fn a_turn_streams_its_updates_in_wire_order_and_settles_on_its_response() {
    let fixture = Fixture::with("prompt-turn");
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

    let turn_id = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");

    let seen = until_settled(&mut stream).await;

    // The turn is announced before anything it produced, and every event that
    // belongs to it names it.
    let started = seen
        .iter()
        .position(|event| event.event == AcpClientEvent::TurnStarted)
        .expect("the turn was announced");
    assert!(
        seen[started..]
            .iter()
            .all(|event| event.turn_id == Some(turn_id))
    );

    // Chunks arrive in the order the agent wrote them, not the order they were
    // convenient to process in.
    let chunks: Vec<_> = seen.iter().filter_map(chunk).collect();
    assert_eq!(chunks, vec!["one".to_owned(), "two".to_owned()]);

    let AcpClientEvent::TurnSettled { stop_reason, .. } =
        seen.last().expect("something was seen").event.clone()
    else {
        panic!("the last event settles the turn: {seen:?}");
    };
    assert_eq!(stop_reason, v1::StopReason::EndTurn);

    // Sequence numbers are dense and increasing, which is what lets a host
    // resume where it left off and know it missed nothing.
    let sequences: Vec<_> = seen.iter().map(|event| event.sequence).collect();
    assert!(
        sequences.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "sequences are not contiguous: {sequences:?}"
    );

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let attachment = &snapshot.attachments[0];
    assert_eq!(attachment.state, AcpAttachmentState::Idle);
    let turn = attachment
        .active_turn
        .as_ref()
        .expect("the turn is recorded");
    assert_eq!(turn.state, AcpTurnState::Settled);
    assert_eq!(turn.stop_reason, Some(v1::StopReason::EndTurn));
    // The settlement is followed by exactly one more event: the chat's activity
    // going idle (#2637-c6d3). It is the newest thing the journal holds.
    let after = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ActivityChanged { .. })
    })
    .await;
    let [idle] = after.as_slice() else {
        panic!("only the activity change follows the settlement: {after:?}");
    };
    let AcpClientEvent::ActivityChanged { activity } = &idle.event else {
        unreachable!()
    };
    assert_eq!(activity.state, AcpChatActivityState::Idle);
    assert_eq!(idle.sequence, *sequences.last().unwrap() + 1);
    assert_eq!(snapshot.latest_sequence, idle.sequence);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A session that finished a turn is free to start the next one.
///
/// The settled turn stays on the attachment, because a host that has just been
/// told a turn ended still has to be able to read how it ended. If holding it
/// also stood in the way, a session would take exactly one turn in its life.
#[tokio::test]
async fn a_session_takes_a_second_turn_once_the_first_one_has_settled() {
    let fixture = Fixture::with("prompt-second-turn");
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

    let first = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("the first prompt");
    let seen = until_settled(&mut stream).await;
    assert_eq!(seen.iter().filter_map(chunk).collect::<Vec<_>>(), ["one"]);

    let second = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("again")],
        )
        .await
        .expect("a settled turn does not stand in the way of the next one");
    assert_ne!(second, first);

    let seen = until_settled(&mut stream).await;
    assert_eq!(seen.iter().filter_map(chunk).collect::<Vec<_>>(), ["two"]);

    // The second turn ends on its own answer, not on a repeat of the first.
    let settled = seen.last().expect("something was seen");
    assert_eq!(settled.turn_id, Some(second));
    let AcpClientEvent::TurnSettled { stop_reason, .. } = settled.event.clone() else {
        panic!("the last event settles the turn: {seen:?}");
    };
    assert_eq!(stop_reason, v1::StopReason::MaxTokens);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let turn = snapshot.attachments[0]
        .active_turn
        .as_ref()
        .expect("the turn is recorded");
    assert_eq!(turn.turn_id, second);
    assert_eq!(turn.stop_reason, Some(v1::StopReason::MaxTokens));

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An answer to a turn the session no longer has is dropped.
///
/// Closing a session with a prompt outstanding leaves an answer with nowhere to
/// land. Loading the session again gives it somewhere that merely looks right:
/// the same id, a fresh attachment, possibly a turn of its own. Applying the
/// old answer there would end a turn the agent has not finished, on a stop
/// reason it gave for something else entirely.
#[tokio::test]
async fn an_answer_to_a_turn_the_session_no_longer_has_is_dropped() {
    let fixture = Fixture::with("prompt-stale-response");
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

    let stale = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("the prompt that is left outstanding");
    fixture
        .manager
        .detach(
            connection.connection_id,
            AcpDetachKind::Close,
            session.session_id.clone(),
        )
        .await
        .expect("session/close");
    fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session.session_id.clone(),
            authority(fixture.root()),
        )
        .await
        .expect("session/load");

    // The agent answers the closed session's prompt before it reads this one,
    // so the answer that must be dropped is already handled by the time this
    // turn ends.
    let fresh = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("again")],
        )
        .await
        .expect("the prompt the reloaded session does run");

    let seen = until_settled(&mut stream).await;
    let settled = seen.last().expect("something was seen");
    assert_eq!(
        settled.turn_id,
        Some(fresh),
        "the first settlement belongs to the live turn: {seen:?}"
    );
    assert!(
        !seen.iter().any(|event| event.turn_id == Some(stale)
            && matches!(event.event, AcpClientEvent::TurnSettled { .. })),
        "an answer to the closed session's turn was applied: {seen:?}"
    );
    assert_eq!(seen.iter().filter_map(chunk).collect::<Vec<_>>(), ["fresh"]);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A cancel reaches a running turn, and does not end it by itself.
///
/// If cancelling settled the turn here, the update the agent is still entitled
/// to send would have nowhere to go, and the client would be asserting an
/// ending the agent never gave — including on the race where the turn had
/// already finished its own way.
#[tokio::test]
async fn a_cancel_reaches_a_running_turn_and_the_response_still_settles_it() {
    let fixture = Fixture::with("prompt-cancel");
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

    // The scenario does not answer the prompt until it has read the cancel, so
    // this call landing at all proves the actor was not stuck awaiting it.
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .expect("session/cancel");

    let seen = until_settled(&mut stream).await;

    // Both chunks are kept: the one before the cancel and the one the agent
    // sent after it, which is still part of what the turn produced.
    let chunks: Vec<_> = seen.iter().filter_map(chunk).collect();
    assert_eq!(chunks, vec!["before".to_owned(), "after".to_owned()]);

    let AcpClientEvent::TurnSettled { stop_reason, .. } =
        seen.last().expect("something was seen").event.clone()
    else {
        panic!("the last event settles the turn: {seen:?}");
    };
    assert_eq!(stop_reason, v1::StopReason::Cancelled);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let turn = snapshot.attachments[0]
        .active_turn
        .as_ref()
        .expect("the turn is recorded");
    assert_eq!(turn.state, AcpTurnState::Settled);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Content the agent cannot read is refused before it is sent.
///
/// Ego advertises image and not audio. An agent that does not understand a
/// block has no way to say so — it answers as though the block were not there —
/// so the caller would be told its recording was considered when it was not.
#[tokio::test]
async fn a_prompt_carrying_unadvertised_content_never_reaches_the_agent() {
    let fixture = Fixture::with("prompt-refused-content");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let audio = v1::ContentBlock::Audio(v1::AudioContent::new("...", "audio/wav"));
    let error = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("listen"), audio],
        )
        .await
        .expect_err("audio this agent never advertised");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert_eq!(error.operation, Some(AcpOperation::PromptAudio));

    // The refusal left nothing running, so the session still takes prompts.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(snapshot.attachments[0].active_turn.is_none());
    assert_eq!(snapshot.attachments[0].state, AcpAttachmentState::Idle);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A cursor the journal no longer holds is a gap, not a quiet resume.
///
/// The events between what the subscriber asked for and what is still held
/// exist nowhere in this client. Handing over what survived would render as a
/// turn where part of what the model said simply never happened, and the host
/// would have no way to know. Recovery is `session/load` against ego, which is
/// the only place that still has it.
#[tokio::test]
async fn a_cursor_the_journal_no_longer_holds_is_reported_as_a_gap() {
    use tuicommander_lib::acp::{AcpConnectionId, AcpEventJournal};

    let connection_id = AcpConnectionId::new();
    let (notices, _) = tokio::sync::broadcast::channel(16);
    let journal = AcpEventJournal::new(connection_id, 1, notices);
    // Comfortably past whatever the journal retains, so the earliest event has
    // certainly been dropped without this test knowing the capacity.
    for _ in 0..10_000 {
        journal.append(None, None, AcpClientEvent::TurnStarted);
    }

    let (earliest, latest) = journal.bounds();
    assert!(earliest > 1, "nothing was dropped: {earliest}..={latest}");

    let error = journal.subscribe(1).err().expect("event 1 is long gone");
    assert_eq!(error.code, AcpClientErrorCode::StreamGap);
    assert!(!error.retryable);

    // The boundary itself is still readable: `earliest` is held, and the event
    // before it is not.
    assert!(journal.subscribe(earliest).is_ok());
    assert!(journal.subscribe(earliest - 1).is_err());

    // Zero means "whatever you still have", which is how a fresh subscriber
    // joins without having to ask what that is first.
    let mut stream = journal.subscribe(0).expect("from the beginning");
    let first = stream.recv().await.expect("held events").expect("no gap");
    assert_eq!(first.sequence, earliest);
    assert_eq!(first.generation, 1);
}

/// A prompt for a session this connection never attached to is refused.
#[tokio::test]
async fn a_prompt_for_an_unattached_session_is_not_found() {
    let fixture = Fixture::with("no-extra-roots");
    let connection = fixture.connect().await;

    let error = fixture
        .manager
        .prompt(
            connection.connection_id,
            v1::SessionId::new(SESSION),
            vec![text("hello")],
        )
        .await
        .expect_err("nothing is attached yet");
    assert_eq!(error.code, AcpClientErrorCode::NotFound);
    assert_eq!(error.session_id, Some(v1::SessionId::new(SESSION)));

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
