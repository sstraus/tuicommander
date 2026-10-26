//! Story092 batch 3: durable session operations across the connection.
//!
//! The connection layer could be proved without ever sending a second request,
//! because everything before this happened inside `initialize`. From here the
//! manager has to reach a live connection *after* `connect` returned, which is
//! the thing the earlier shape could not do at all: the SDK connection lived
//! only inside the supervisor closure and nothing outside could speak on it.
//!
//! Ego owns durable session identity, history, lineage and state. What these
//! tests are about is the client half — that a returned durable id becomes an
//! attachment carrying the authority it was created with, that an operation the
//! agent never advertised is refused before a byte is written, and that a
//! refusal leaves nothing attached.

use agent_client_protocol::schema::v1;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, layer::Context, prelude::*};
use tuicommander_lib::acp::{
    AcpAttachKind, AcpAttachmentState, AcpClientErrorCode, AcpDetachKind, AcpOperation,
    AcpSessionAuthority,
};

use crate::acp_support;

use acp_support::{Fixture, authority, text, until};

/// The session ids the scenarios answer with, spelled once.
const FIRST: &str = "01932d5e-0000-7000-8000-0000000000aa";
const FORKED: &str = "01932d5e-0000-7000-8000-0000000000bb";

fn session(id: &str) -> v1::SessionId {
    v1::SessionId::new(id)
}

#[derive(Default)]
struct CapturedFields(BTreeMap<String, String>);

impl Visit for CapturedFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

struct AttachLogLayer(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

impl<S: tracing::Subscriber> Layer<S> for AttachLogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = CapturedFields::default();
        event.record(&mut fields);
        if fields
            .0
            .get("message")
            .is_some_and(|text| text.contains("ACP attach"))
        {
            self.0.lock().unwrap().push(fields.0);
        }
    }
}

/// A future initialize storm needs its initiating ACP method and session id
/// in the host log, not just a count of new MCP sessions.
#[tokio::test]
async fn every_attach_log_names_its_method_and_session() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(AttachLogLayer(events.clone()));
    tracing::subscriber::set_global_default(subscriber).expect("install test log collector");

    let fixture = Fixture::with("no-lifecycle");
    let connection = fixture.connect().await;
    for (kind, expected_method) in [
        (AcpAttachKind::Load, "session/load"),
        (AcpAttachKind::Resume, "session/resume"),
        (AcpAttachKind::Fork, "session/fork"),
    ] {
        let error = fixture
            .manager
            .attach(
                connection.connection_id,
                kind,
                session(FIRST),
                authority(fixture.root()),
            )
            .await
            .expect_err("fixture advertises no attach capabilities");
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        let rows = events.lock().unwrap();
        assert!(
            rows.iter().any(|fields| {
                fields.get("method").map(|value| value.trim_matches('"')) == Some(expected_method)
                    && fields
                        .get("session_id")
                        .map(|value| value.trim_matches('"'))
                        == Some(FIRST)
                    && fields.get("source").map(|value| value.trim_matches('"')) == Some("acp")
            }),
            "missing structured attach log for {expected_method} and {FIRST}: {rows:?}"
        );
    }
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_new_session_attaches_the_durable_id_the_agent_returned() {
    let fixture = Fixture::with("session-new");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let attachment = fixture
        .manager
        .new_session(connection.connection_id, authority(root.clone()))
        .await
        .expect("session/new");

    assert_eq!(attachment.state, AcpAttachmentState::Idle);
    assert_eq!(attachment.cwd, root);
    assert!(attachment.active_turn.is_none());
    assert_eq!(
        attachment.pending_permission_ids,
        [] as [tuicommander_lib::acp::AcpHostRequestId; 0]
    );

    // The attachment is connection state, so the connection snapshot carries it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![attachment.clone()]);

    let listed = fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect("session/list");
    assert_eq!(listed.sessions.len(), 1);
    assert_eq!(listed.sessions[0].session_id, attachment.session_id);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An unadvertised operation is refused before the wire, not after it.
///
/// The scenario asserts the silence: its last step reads stdin and fails if
/// anything arrived. A client that optimistically sent `session/list` and let
/// the agent say `method_not_found` would pass a test that only looked at the
/// error, and would have told the agent about a session it has no business
/// knowing.
#[tokio::test]
async fn an_unadvertised_operation_is_refused_without_writing_a_byte() {
    let fixture = Fixture::with("no-lifecycle");
    let connection = fixture.connect().await;
    let capabilities = connection.capabilities.as_ref().expect("negotiated");
    assert!(!capabilities.availability(AcpOperation::List).available);

    let error = fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect_err("an unadvertised list is refused");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert_eq!(error.operation, Some(AcpOperation::List));
    assert!(!error.retryable);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Extra roots are refused when the agent never said it honours them.
///
/// `session/new` itself is baseline, so nothing stops the request from going
/// out — which is the danger. An agent that does not know the field ignores it
/// and answers with a session, and the caller is handed a session it believes
/// spans three directories while the agent will only ever touch one. Ego is
/// exactly such an agent: its advertised `sessionCapabilities` carry no
/// `additionalDirectories`.
#[tokio::test]
async fn extra_roots_are_refused_when_the_agent_never_advertised_them() {
    let fixture = Fixture::with("no-extra-roots");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let error = fixture
        .manager
        .new_session(
            connection.connection_id,
            AcpSessionAuthority {
                cwd: root.clone(),
                additional_directories: vec![root.join("elsewhere")],
                mcp_servers: Vec::new(),
            },
        )
        .await
        .expect_err("extra roots this agent never advertised");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert_eq!(error.operation, Some(AcpOperation::AdditionalDirectories));
    assert!(!error.retryable);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_refused_new_session_leaves_the_connection_with_no_attachment() {
    let fixture = Fixture::with("refuses-new");
    let connection = fixture.connect().await;

    let error = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect_err("the agent refused");
    assert_eq!(error.code, AcpClientErrorCode::AgentError);
    assert_eq!(error.connection_id, Some(connection.connection_id));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a refused session was attached anyway: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Loading attaches a session ego already owns; closing lets go of it.
///
/// The id is not the agent's to choose here — it is the one the caller named,
/// and a client that took the durable id from anywhere but the caller's request
/// would attach to a session nobody asked for.
#[tokio::test]
async fn a_loaded_session_attaches_under_the_id_that_was_asked_for() {
    let fixture = Fixture::with("session-attach");
    let connection = fixture.connect().await;

    let attachment = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FIRST),
            authority(fixture.root()),
        )
        .await
        .expect("session/load");
    assert_eq!(attachment.session_id, session(FIRST));
    assert_eq!(attachment.state, AcpAttachmentState::Idle);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![attachment]);

    fixture
        .manager
        .detach(
            connection.connection_id,
            AcpDetachKind::Close,
            session(FIRST),
        )
        .await
        .expect("session/close");

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a closed session stayed attached: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A replay is already on the wire while session/load is awaiting its reply.
/// Dropping those early chunks makes the first assistant answer start midway.
#[tokio::test]
async fn load_preserves_the_first_replayed_assistant_chunks() {
    let fixture = Fixture::with("session-load-early-chunks");
    let connection = fixture.connect().await;

    fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FIRST),
            authority(fixture.root()),
        )
        .await
        .expect("session/load");

    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("journal");
    // The scenario sends its last chunk after the load response, so the journal may
    // place it after `Idle`. Stopping at `Idle` made this test race the host's
    // handling of the response against the following notification (~20% of runs
    // on the Linux box). Read until the whole replay has arrived instead.
    let expected = "TUICommander v1.7.7 is connected.\nintent: Checking active agents (Agents)";
    let answer = std::cell::RefCell::new(String::new());
    let events = until(&mut stream, |event| {
        if let tuicommander_lib::acp::AcpClientEvent::SessionUpdate { update } = event
            && let v1::SessionUpdate::AgentMessageChunk(chunk) = &**update
            && let v1::ContentBlock::Text(text) = &chunk.content
        {
            answer.borrow_mut().push_str(&text.text);
        }
        answer.borrow().len() >= expected.len()
    })
    .await;
    let answer = answer.into_inner();
    assert_eq!(answer, expected, "events: {events:?}");

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A fork is a second session, and the original keeps its own attachment.
///
/// Ego owns the lineage; what the client must not do is treat the fork's new
/// durable id as a rename of the one it forked from. Losing the original here
/// would silently detach a session that is still perfectly alive.
#[tokio::test]
async fn a_fork_attaches_the_new_id_and_keeps_the_original() {
    let fixture = Fixture::with("session-fork");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let original = fixture
        .manager
        .new_session(connection.connection_id, authority(root.clone()))
        .await
        .expect("session/new");
    assert_eq!(original.session_id, session(FIRST));

    let fork = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Fork,
            session(FIRST),
            authority(root),
        )
        .await
        .expect("session/fork");
    assert_eq!(fork.session_id, session(FORKED));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![original, fork]);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_resumed_session_attaches_and_a_deleted_one_detaches() {
    let fixture = Fixture::with("session-resume-delete");
    let connection = fixture.connect().await;

    let attachment = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Resume,
            session(FIRST),
            authority(fixture.root()),
        )
        .await
        .expect("session/resume");
    assert_eq!(attachment.session_id, session(FIRST));

    fixture
        .manager
        .detach(
            connection.connection_id,
            AcpDetachKind::Delete,
            session(FIRST),
        )
        .await
        .expect("session/delete");

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a deleted session stayed attached: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Attaching to a session this connection already holds is refused.
///
/// Not tidiness. The attachment is where the running turn, the usage totals and
/// the ids of the questions a person has open all live, and attaching writes a
/// fresh one: a second attach would blank the turn, and the response that
/// settles it would then arrive for a turn the attachment no longer names and
/// be dropped as stale. A host would be left watching a turn that never ends.
/// The seats, meanwhile, outlive the overwrite, so the attachment would go on
/// reporting no pending questions while the agent waits on two.
///
/// Refused rather than merged, because there is nothing a second attach can add:
/// this connection already has the session. A host that wants the history
/// replayed detaches first, which says what it means.
///
/// Only load and resume can collide. A fork names the session it forks *from*
/// and comes back with an id of its own, so it is a second session rather than
/// a second attach to one, and nothing here refuses it.
#[tokio::test]
async fn a_session_this_connection_already_holds_is_not_attached_twice() {
    let fixture = Fixture::with("session-attach-twice");
    let connection = fixture.connect().await;
    let root = fixture.root();

    fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FIRST),
            authority(root.clone()),
        )
        .await
        .expect("session/load");
    let turn = fixture
        .manager
        .prompt(
            connection.connection_id,
            session(FIRST),
            vec![text("what is going on")],
        )
        .await
        .expect("session/prompt");

    for kind in [AcpAttachKind::Load, AcpAttachKind::Resume] {
        let error = fixture
            .manager
            .attach(
                connection.connection_id,
                kind,
                session(FIRST),
                authority(root.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, AcpClientErrorCode::InvalidInput, "{kind:?}");
        assert_eq!(error.session_id, Some(session(FIRST)), "{kind:?}");
    }

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let attachment = snapshot.attachments.first().expect("still attached");
    assert_eq!(
        attachment.active_turn.as_ref().map(|active| active.turn_id),
        Some(turn),
        "the running turn survived the refused attach"
    );
    assert_eq!(attachment.state, AcpAttachmentState::Prompting);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A session whose attach is still pending is not attached a second time.
///
/// The already-attached check alone leaves a window: until the agent answers
/// the first `session/load` there is no attachment to collide with, so a host
/// that asked again — a panel re-rendering, a retry loop — sent another load,
/// and ego admits every MCP server again for each one. The scenario leaves the
/// first load unanswered and fails if a second frame reaches it.
#[tokio::test]
async fn a_session_with_a_pending_attach_is_not_attached_twice() {
    let fixture = Fixture::with("session-attach-pending");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let mut first = Box::pin(fixture.manager.attach(
        connection.connection_id,
        AcpAttachKind::Load,
        session(FIRST),
        authority(root.clone()),
    ));
    for kind in [AcpAttachKind::Load, AcpAttachKind::Resume] {
        let second = fixture.manager.attach(
            connection.connection_id,
            kind,
            session(FIRST),
            authority(root.clone()),
        );
        // Biased, so the first load is queued before the second is.
        let error = tokio::select! {
            biased;
            settled = &mut first => panic!("a second attach reached the agent: {settled:?}"),
            refused = second => refused.unwrap_err(),
        };
        assert_eq!(
            error.code,
            AcpClientErrorCode::InvalidInput,
            "{kind:?}: {error:?}"
        );
        assert_eq!(error.session_id, Some(session(FIRST)), "{kind:?}");
    }

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
    first
        .await
        .expect_err("an unanswered load ends with the connection");
}

/// Every lifecycle operation is gated on the capability that names it.
///
/// One test per operation would prove the same thing five times; what matters
/// is that no operation was left ungated, which is a statement about the set.
#[tokio::test]
async fn no_lifecycle_operation_reaches_an_agent_that_advertised_none() {
    let fixture = Fixture::with("no-lifecycle");
    let connection = fixture.connect().await;
    let root = fixture.root();

    for (kind, operation) in [
        (AcpAttachKind::Load, AcpOperation::Load),
        (AcpAttachKind::Fork, AcpOperation::Fork),
        (AcpAttachKind::Resume, AcpOperation::Resume),
    ] {
        let error = fixture
            .manager
            .attach(
                connection.connection_id,
                kind,
                session(FIRST),
                authority(root.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        assert_eq!(error.operation, Some(operation));
    }

    for (kind, operation) in [
        (AcpDetachKind::Close, AcpOperation::Close),
        (AcpDetachKind::Delete, AcpOperation::Delete),
    ] {
        let error = fixture
            .manager
            .detach(connection.connection_id, kind, session(FIRST))
            .await
            .unwrap_err();
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        assert_eq!(error.operation, Some(operation));
    }

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A settled connection admits no further session work.
///
/// Reaching a dead process would either hang on a reply that never comes or
/// look like a fresh failure of the operation. It is neither: the connection is
/// gone, and the caller has to make a new one.
#[tokio::test]
async fn a_settled_connection_refuses_session_work_as_transport_closed() {
    let fixture = Fixture::with("session-new");
    let connection = fixture.connect().await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();

    let error = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect_err("a settled connection carries no session");
    assert_eq!(error.code, AcpClientErrorCode::TransportClosed);
}

// ---------------------------------------------------------------------------
// The one MCP server a session is given
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_acp_peer_keeps_one_identity_in_ego_and_its_conversation() {
    let fixture = Fixture::with("session-new-peer-mcp");
    let peer = "550e8400-e29b-41d4-a716-446655440a01";
    let connection = fixture
        .manager
        .connect_with_peer(
            &Fixture::config(),
            tuicommander_lib::acp::AcpConnectRequest {
                root: fixture.root(),
            },
            peer.to_string(),
        )
        .await
        .expect("connect as ACP peer");

    assert_eq!(fixture.manager.peer_root(peer), Some(fixture.root()));
    let first = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("new session");
    assert_eq!(first.session_id, session(FIRST));
    assert_eq!(
        fixture.manager.peer_conversation(peer),
        Some((fixture.root(), session(FIRST)))
    );

    fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FORKED),
            authority(fixture.root()),
        )
        .await
        .expect("load session");
    assert_eq!(
        fixture.manager.peer_conversation(peer),
        Some((fixture.root(), session(FORKED)))
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
    assert!(fixture.manager.peer_conversation(peer).is_none());
}

/// Every session carries TUICommander, and carries nothing a caller named.
///
/// Both halves of plan §4.5 in one scenario, because they are one rule: the
/// list is built here from the server this process serves, so the intruder
/// below is not filtered out of it — it is never consulted. `session/load` is in the
/// same scenario for the same reason `start_attach` shares a body with
/// `start_new_session`: an entry synthesised for one and forgotten for the
/// other is a session that can reach nothing, found only by a person.
///
/// The intruder is an HTTP entry on purpose: replacement has to hold for a
/// transport the grant no longer uses, or the test would pass on a list that
/// merely filtered by shape.
#[tokio::test]
async fn every_session_carries_this_process_and_nothing_a_caller_named() {
    let fixture = Fixture::with("session-new-tuic-mcp");
    let connection = fixture.connect().await;

    let intruder = AcpSessionAuthority {
        cwd: fixture.root(),
        additional_directories: Vec::new(),
        mcp_servers: vec![v1::McpServer::Http(v1::McpServerHttp::new(
            "intruder",
            "http://intruder.example/mcp",
        ))],
    };

    fixture
        .manager
        .new_session(connection.connection_id, intruder.clone())
        .await
        .expect("session/new");

    fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FORKED),
            intruder,
        )
        .await
        .expect("session/load");

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
