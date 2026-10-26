//! Story092 batch 5a: the questions the agent asks back.
//!
//! Everything else on this connection is the client asking and the agent
//! answering. These two methods run the other way, and they are the only place
//! where a *person* is on the critical path: the agent stops, and stays
//! stopped, until someone decides.
//!
//! That is what makes the seat the unit here. The SDK hands the responder to a
//! callback that holds the dispatch loop, so answering there would freeze the
//! connection for as long as the person takes — including the updates that say
//! what they are being asked about. The responder is therefore carried out of
//! the callback and parked, and what is proved below is that it is always
//! answered exactly once: by the person, by the cancel that took the question
//! away from them, or immediately when there was no seat to offer.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientEvent, AcpEventEnvelope, AcpHostRequestId, AcpPendingInteraction,
};

use crate::acp_support;

use acp_support::{Fixture, authority, chunk, text, until};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";
const OTHER_SESSION: &str = "01932d5e-0000-7000-8000-0000000000bb";

fn selected(option: &str) -> v1::RequestPermissionOutcome {
    v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
        v1::PermissionOptionId::new(option),
    ))
}

/// A filled-in form, in the one shape the `elicitation-turn` scenario expects.
fn accepted(branch: &str) -> v1::ElicitationAction {
    let mut accept = v1::ElicitationAcceptAction::new();
    accept.content = Some([("branch".to_owned(), branch.into())].into_iter().collect());
    v1::ElicitationAction::Accept(accept)
}

/// The id the agent's question was seated under.
///
/// Reading it off the stream rather than off a snapshot is deliberate: the
/// stream is the channel a frontend actually renders from, and an id it cannot
/// see there is one it cannot answer with.
fn asked(seen: &[AcpEventEnvelope]) -> AcpHostRequestId {
    seen.iter()
        .find_map(|event| match &event.event {
            AcpClientEvent::PermissionRequested { request_id, .. }
            | AcpClientEvent::ElicitationRequested { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no question was seated: {seen:?}"))
}

/// A person's answer reaches the agent, and the turn goes on around it.
///
/// The two chunks either side of the question are the point: the client is not
/// blocked while a person thinks, and what the agent said before it asked is
/// filed before the asking, not after.
#[tokio::test]
async fn a_permission_is_seated_answered_and_the_answer_reaches_the_agent() {
    let fixture = Fixture::with("permission-turn");
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

    let asking = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PermissionRequested { .. })
    })
    .await;
    let request_id = asked(&asking);
    assert_eq!(
        asking.iter().filter_map(chunk).collect::<Vec<_>>(),
        ["before"],
        "what the agent said before it asked is filed before the asking"
    );

    // The question is on the attachment too, so a frontend that was not
    // listening when it was asked still finds it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        vec![request_id]
    );
    let pending = fixture
        .manager
        .pending_interactions(connection.connection_id)
        .await
        .expect("pending interactions");
    let [
        AcpPendingInteraction::Permission {
            request_id: listed,
            session_id,
            request,
        },
    ] = pending.as_slice()
    else {
        panic!("exactly one permission is open: {pending:?}");
    };
    assert_eq!(*listed, request_id);
    assert_eq!(session_id.0.as_ref(), SESSION);
    assert_eq!(request.tool_call.tool_call_id.0.as_ref(), "call-1");

    let settlement = fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the answer is accepted");
    assert_eq!(settlement.request_id, request_id);

    // The scenario asserts the wire answer itself; a turn that ends proves the
    // agent read it, because the fixture will not go on until it has.
    let rest = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert!(matches!(
        rest.first().map(|event| &event.event),
        Some(AcpClientEvent::PermissionSettled { .. })
    ));
    assert_eq!(rest.iter().filter_map(chunk).collect::<Vec<_>>(), ["after"]);

    // A settled seat is gone from both places that listed it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions"),
        [] as [tuicommander_lib::acp::AcpPendingInteraction; 0]
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An answered question leaves the attachment before the agent speaks again.
///
/// The sibling test above answers a question mid-turn and the agent keeps
/// talking, and every update it sends republishes the attachment. That makes it
/// blind to the bug this one is for: a client that dropped the question from
/// its own state but never told a host would still look right there, repaired
/// by the next chunk before anybody could read it.
///
/// So here the agent goes silent the moment it has its answer. Nothing else can
/// publish, and the attachment either shows the question gone because answering
/// published it or shows it still open. A host polls this to decide what to put
/// in front of a person, and a question that has already been decided is the
/// one thing it must not still be asking.
#[tokio::test]
async fn an_answered_question_leaves_the_attachment_before_the_agent_speaks_again() {
    let fixture = Fixture::with("permission-then-silence");
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

    let request_id = asked(
        &until(&mut stream, |event| {
            matches!(event, AcpClientEvent::PermissionRequested { .. })
        })
        .await,
    );
    let seated = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        seated.attachments[0].pending_permission_ids,
        vec![request_id],
        "the question has to be listed first for its removal to mean anything"
    );

    fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the answer is accepted");

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments[0].pending_permission_ids.is_empty(),
        "the answered question was still listed, and nothing was coming to fix it: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An option the agent never offered is refused here, and the seat stays open.
///
/// Refusing locally rather than forwarding matters because the agent has to
/// treat the answer as authoritative. Leaving the seat open matters more: a
/// frontend that sent a stale option id gets to try again, instead of having
/// silently cancelled the question on the person's behalf.
#[tokio::test]
async fn an_option_the_agent_never_offered_is_refused_and_the_seat_stays_open() {
    let fixture = Fixture::with("permission-turn");
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
    let request_id = asked(
        &until(&mut stream, |event| {
            matches!(event, AcpClientEvent::PermissionRequested { .. })
        })
        .await,
    );

    let error = fixture
        .manager
        .respond_permission(
            connection.connection_id,
            request_id,
            selected("allow-always"),
        )
        .await
        .expect_err("an option that was never offered is not an answer");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);
    assert!(
        error.message.contains("allow-always"),
        "the refusal names the option it refused: {error:?}"
    );

    // Still open, and still answerable.
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions")
            .len(),
        1
    );
    fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the seat was still there to answer");

    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;

    // And answering it twice is not a second answer.
    let error = fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect_err("a settled question cannot be answered again");
    assert_eq!(error.code, AcpClientErrorCode::NotFound);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An elicitation in a mode this client never advertised is declined, not seated.
///
/// `initialize` offers `elicitation.form` and nothing else. A `url` mode asks
/// the client to send a person to an address instead of drawing a form, and the
/// protocol says in as many words that a client which does not understand a
/// mode must not render it as one it does. Seating it is exactly that: the
/// request lands on the attachment and a frontend draws the form it knows how
/// to draw, from a request whose only real content — the address — has no place
/// in one. The person is then asked to decide something they were never shown.
///
/// So the answer is the one a question with no seat gets. The agent is told
/// immediately, in the vocabulary it asked in, and it is told by the client
/// that declared the capability it just ignored.
#[tokio::test]
async fn an_elicitation_in_an_unadvertised_mode_is_declined_rather_than_seated() {
    let fixture = Fixture::with("elicitation-url-mode");
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
        matches!(event, AcpClientEvent::ElicitationSettled { .. })
    })
    .await;

    // On the record even though nobody could act on it: a host is owed the fact
    // that the agent asked for something this client does not do.
    let requested = seen
        .iter()
        .find_map(|event| match &event.event {
            AcpClientEvent::ElicitationRequested { request, .. } => {
                Some((event.session_id.as_ref(), request))
            }
            _ => None,
        })
        .expect("the unadvertised elicitation was recorded");
    // Asserted rather than assumed. A scenario whose mode quietly failed to
    // parse would be declined for having no scope at all, and would prove
    // nothing about the mode this test is named for.
    assert!(
        matches!(requested.1.mode, v1::ElicitationMode::Url(_)),
        "the scenario has to actually deliver a url mode: {:?}",
        requested.1.mode
    );
    assert_eq!(
        requested.0.map(|id| id.0.as_ref()),
        Some(SESSION),
        "it named a session this connection holds, which is why the mode is the only thing wrong"
    );
    let settled = seen
        .iter()
        .find_map(|event| match &event.event {
            AcpClientEvent::ElicitationSettled { action, .. } => Some(action),
            _ => None,
        })
        .expect("and the answer it got is on the record too");
    assert!(matches!(settled, v1::ElicitationAction::Cancel));

    // Nothing was seated, so no person is being asked and no attachment lists it.
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions"),
        [] as [tuicommander_lib::acp::AcpPendingInteraction; 0]
    );
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_elicitation_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An elicitation action outside the protocol's three is refused here.
///
/// The same rule as the unoffered option id above, on the other seat. ACP has
/// exactly three actions — accept, decline, cancel — and anything else
/// deserialises into the catch-all the schema keeps for extensions and future
/// versions. A frontend can put one there over HTTP, and forwarding it would
/// mean answering the agent in a vocabulary this client cannot know it speaks.
///
/// It also costs the person their question. Answering consumes the seat, so an
/// action the agent then rejects leaves nobody able to try again: the form is
/// gone from this client and the agent is left holding an error instead of a
/// decision. Refusing before the seat is taken keeps the question answerable,
/// which is the whole reason the permission path refuses rather than forwards.
///
/// Not a guess about the agent, either. The one this client launches maps the
/// catch-all straight to `unsupported elicitation action` — so refusing here
/// costs nothing that would otherwise have worked, and saves the seat.
#[tokio::test]
async fn an_elicitation_action_outside_the_protocol_is_refused_and_the_seat_stays_open() {
    let fixture = Fixture::with("elicitation-turn");
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
    let request_id = asked(
        &until(&mut stream, |event| {
            matches!(event, AcpClientEvent::ElicitationRequested { .. })
        })
        .await,
    );

    let invented = v1::ElicitationAction::Other(v1::OtherElicitationAction::new(
        "_tuic/approve",
        Default::default(),
    ));
    let error = fixture
        .manager
        .respond_elicitation(connection.connection_id, request_id, invented)
        .await
        .expect_err("an action the protocol does not define is not an answer");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);
    assert!(
        error.message.contains("_tuic/approve"),
        "the refusal names the action it refused: {error:?}"
    );

    // Still open, and still answerable.
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions")
            .len(),
        1
    );
    fixture
        .manager
        .respond_elicitation(connection.connection_id, request_id, accepted("main"))
        .await
        .expect("the seat was still there to answer");

    // The scenario asserts the wire answer; a turn that ends proves the agent
    // read the accept and not the invention.
    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Cancelling a turn answers every question that turn was waiting on.
///
/// ACP requires the `Cancelled` outcome for pending permissions on cancel, and
/// the same reasoning covers elicitations: a person is not going to answer a
/// question belonging to a turn that is being abandoned, and an unanswered
/// request would leave the agent waiting on it forever.
#[tokio::test]
async fn cancelling_a_turn_settles_every_question_it_was_waiting_on() {
    let fixture = Fixture::with("permission-cancel");
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

    let asking = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ElicitationRequested { .. })
    })
    .await;
    let mut open = asking.iter().filter_map(|event| match &event.event {
        AcpClientEvent::PermissionRequested { request_id, .. }
        | AcpClientEvent::ElicitationRequested { request_id, .. } => Some(*request_id),
        _ => None,
    });
    let permission = open.next().expect("the permission was asked first");
    let elicitation = open.next().expect("the elicitation was asked second");
    assert_ne!(permission, elicitation);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        vec![permission]
    );
    assert_eq!(
        snapshot.attachments[0].pending_elicitation_ids,
        vec![elicitation]
    );

    // The scenario reads the two answers before `session/cancel`, so a client
    // that told the agent to stop while still holding a question would fail
    // there rather than here.
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .expect("session/cancel");

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    let settlements: Vec<_> = seen
        .iter()
        .filter_map(|event| match &event.event {
            AcpClientEvent::PermissionSettled {
                request_id,
                outcome,
            } => {
                assert_eq!(*outcome, v1::RequestPermissionOutcome::Cancelled);
                Some(*request_id)
            }
            AcpClientEvent::ElicitationSettled { request_id, action } => {
                assert!(matches!(action, v1::ElicitationAction::Cancel));
                Some(*request_id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        settlements,
        vec![permission, elicitation],
        "the sweep answers in the order the agent asked"
    );

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );
    assert_eq!(
        snapshot.attachments[0].pending_elicitation_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A question this client has no seat for is declined without a person.
///
/// Both cases name nothing this client could render against: an elicitation
/// scoped to a JSON-RPC request belongs to the phase before a session exists,
/// and a permission for a session this connection is not attached to has no
/// view to appear in. Holding either would leave the agent waiting on a human
/// who is never going to be shown the question.
#[tokio::test]
async fn a_question_with_no_seat_is_declined_rather_than_held() {
    let fixture = Fixture::with("interaction-unseatable");
    let connection = fixture.connect().await;
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PermissionSettled { .. })
    })
    .await;

    // Both halves are on the record even though nobody could act on them: a
    // host is owed the fact that the agent asked, and that the answer it got
    // was not a person's.
    let elicitation = seen
        .iter()
        .find(|event| matches!(event.event, AcpClientEvent::ElicitationRequested { .. }))
        .expect("the unscoped elicitation was recorded");
    assert_eq!(
        elicitation.session_id, None,
        "an elicitation outside a session is attributed to none"
    );
    let permission = seen
        .iter()
        .find(|event| matches!(event.event, AcpClientEvent::PermissionRequested { .. }))
        .expect("the permission for an unattached session was recorded");
    assert_eq!(
        permission.session_id.as_ref().map(|id| id.0.as_ref()),
        Some(OTHER_SESSION),
        "a permission keeps the session it named, attached or not"
    );

    // Nothing was ever seated, so nothing is waiting on a person.
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions"),
        [] as [tuicommander_lib::acp::AcpPendingInteraction; 0]
    );
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );
    assert_eq!(
        snapshot.attachments[0].pending_elicitation_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
