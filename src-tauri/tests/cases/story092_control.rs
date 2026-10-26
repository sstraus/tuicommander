//! Story092 batch 5b: the controls that steer a session rather than drive it.
//!
//! `session/set_config_option` is ACP's; `_ego/pause`, `_ego/resume` and
//! `_ego/compact` are ego's, advertised under `agentCapabilities._meta.ego` and
//! present only because ego is on the other end. All four share one property
//! that the tests below are built around: the agent is the authority, and this
//! client's job is to refuse locally exactly what the agent could never honour,
//! then to record what the agent said without softening it.
//!
//! Two answers in particular are not failures and must not be treated as such.
//! A hold that reports `pending` means the request is on record and the turn
//! has not reached a boundary yet — only `paused` says nothing more runs. And a
//! compaction whose publication is uncertain is not a compaction to retry: a
//! second attempt risks a second successor for one source.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpAttachmentState, AcpClientErrorCode, AcpClientEvent, AcpEventEnvelope, AcpHoldState,
    AcpTargetPublication, EgoCompactRequest, EgoHoldRequest,
};

use crate::acp_support;

use acp_support::{Fixture, authority, text, until};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";
const SUCCESSOR: &str = "01932d5e-0000-7000-8000-0000000000cc";
/// The request ids the scenarios pin, so the wire is asserted against them.
const HOLD_REQUEST: &str = "01932d5e-0000-7000-8000-0000000000f1";
const COMPACT_REQUEST: &str = "01932d5e-0000-7000-8000-0000000000f2";

fn config(
    session_id: &v1::SessionId,
    id: &str,
    value: v1::SessionConfigOptionValue,
) -> v1::SetSessionConfigOptionRequest {
    v1::SetSessionConfigOptionRequest::new(session_id.clone(), v1::SessionConfigId::new(id), value)
}

fn value(id: &str) -> v1::SessionConfigOptionValue {
    v1::SessionConfigOptionValue::value_id(v1::SessionConfigValueId::new(id))
}

/// The ids of a config set, in the order the agent listed them.
fn ids(options: &[v1::SessionConfigOption]) -> Vec<&str> {
    options.iter().map(|option| option.id.0.as_ref()).collect()
}

/// The current value of a select, or `None` if that option is not one.
fn current(options: &[v1::SessionConfigOption], id: &str) -> Option<String> {
    let option = options.iter().find(|option| option.id.0.as_ref() == id)?;
    match &option.kind {
        v1::SessionConfigKind::Select(select) => Some(select.current_value.0.to_string()),
        _ => None,
    }
}

/// Every attachment state the stream reported, in order.
fn states(seen: &[AcpEventEnvelope]) -> Vec<AcpAttachmentState> {
    seen.iter()
        .filter_map(|event| match event.event {
            AcpClientEvent::AttachmentState { state } => Some(state),
            _ => None,
        })
        .collect()
}

fn hold(session_id: &v1::SessionId, request_id: &str) -> EgoHoldRequest {
    EgoHoldRequest {
        session_id: session_id.clone(),
        request_id: request_id.parse().expect("a request id"),
    }
}

/// One option is set, the whole set comes back, and the refusals never fly.
///
/// The refusals are asserted by what the scenario reads next: each of them
/// happens before the one valid call, and the scenario's next `expect` is that
/// valid call. A client that had forwarded any of them would be caught there,
/// naming the frame it should never have written.
#[tokio::test]
async fn one_option_is_set_the_whole_set_comes_back_and_the_refused_ones_never_fly() {
    let fixture = Fixture::with("config-set");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id.clone();
    assert_eq!(ids(&session.config_options), ["model", "effort", "verbose"]);

    // An option this session never offered.
    let error = fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(&session_id, "nonesuch", value("whatever")),
        )
        .await
        .expect_err("an option that was never offered is not settable");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert!(
        error.message.contains("nonesuch"),
        "the refusal names the option it refused: {error:?}"
    );

    // A value outside the offer this option published.
    let error = fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(&session_id, "model", value("haiku")),
        )
        .await
        .expect_err("a value the offer does not list has no meaning to send");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert!(
        error.message.contains("haiku"),
        "the refusal names the value it refused: {error:?}"
    );

    // A boolean option: excluded by this client's own contract, because nothing
    // here renders or answers one.
    let error = fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(
                &session_id,
                "verbose",
                v1::SessionConfigOptionValue::boolean(true),
            ),
        )
        .await
        .expect_err("this client does not claim a seat for boolean options");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert!(
        error.message.contains("ClientBooleanConfig"),
        "the refusal names the contract that excludes it: {error:?}"
    );

    // A select handed the wrong shape of value.
    let error = fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(
                &session_id,
                "model",
                v1::SessionConfigOptionValue::boolean(true),
            ),
        )
        .await
        .expect_err("a select takes a value id");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);

    // The one that is real, and on the grouped select: a client that only
    // walked the flat list would refuse a value the agent really did offer.
    let options = fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(&session_id, "effort", value("high")),
        )
        .await
        .expect("session/set_config_option");

    // The whole set is taken, not the one option that was set: `verbose` is
    // gone and `model` came back narrowed, and both are the agent's word on
    // what setting `effort` did.
    assert_eq!(ids(&options), ["model", "effort"]);
    assert_eq!(current(&options, "effort").as_deref(), Some("high"));
    assert_eq!(current(&options, "model").as_deref(), Some("opus"));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].config_options, options,
        "the attachment holds exactly what the caller was told"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A pause that has not landed is `pending`, and a resume puts the turn back.
///
/// Both are reported as ego reported them. `pending` is the interesting one: it
/// means the hold is recorded and the turn is still running, so a host that
/// rendered it as "paused" would be telling a person that nothing more will
/// happen while the model is still producing output.
#[tokio::test]
async fn a_pause_that_has_not_landed_is_pending_and_a_resume_puts_the_turn_back() {
    let fixture = Fixture::with("ego-hold");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id.clone();

    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");

    // A nil request id is the value ego rejects outright, and the one a caller
    // reaches by default-constructing rather than deciding. Refused before the
    // write, so the scenario's next frame is still the real pause.
    let error = fixture
        .manager
        .pause_turn(
            connection.connection_id,
            EgoHoldRequest {
                session_id: session_id.clone(),
                request_id: uuid::Uuid::nil(),
            },
        )
        .await
        .expect_err("a nil request id names no attempt");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);

    let paused = fixture
        .manager
        .pause_turn(connection.connection_id, hold(&session_id, HOLD_REQUEST))
        .await
        .expect("_ego/pause");
    assert_eq!(paused.state, AcpHoldState::Pending);
    assert_eq!(paused.session_id, session_id);
    assert_eq!(paused.request_id.to_string(), HOLD_REQUEST);

    // The same request id, because a resume that names a fresh one is a
    // different attempt and ego would treat it as such.
    let resumed = fixture
        .manager
        .resume_turn(connection.connection_id, hold(&session_id, HOLD_REQUEST))
        .await
        .expect("_ego/resume");
    assert_eq!(resumed.state, AcpHoldState::Running);

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert_eq!(
        states(&seen),
        [
            AcpAttachmentState::Idle,
            AcpAttachmentState::PausePending,
            AcpAttachmentState::Prompting,
        ],
        "a released hold goes back to the turn that was running, not to idle"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A compaction whose publication is uncertain is a receipt, not a retry.
///
/// The successor is a different session that this connection is not attached
/// to, so nothing here changes: attaching to it is a separate decision, and the
/// receipt is what a host makes it with.
#[tokio::test]
async fn a_compaction_with_an_uncertain_publication_is_never_retry_safe() {
    let fixture = Fixture::with("ego-compact");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id.clone();

    let receipt = fixture
        .manager
        .compact(
            connection.connection_id,
            EgoCompactRequest {
                session_id: session_id.clone(),
                request_id: COMPACT_REQUEST.parse().expect("a request id"),
            },
        )
        .await
        .expect("_ego/compact");

    assert_eq!(receipt.source_session_id, session_id);
    assert_eq!(receipt.source_seq, 41);
    assert_eq!(receipt.target_session_id.0.as_ref(), SUCCESSOR);
    assert!(matches!(
        receipt.publication,
        AcpTargetPublication::PublishedDurabilityUncertain { .. }
    ));
    assert!(
        !receipt.publication.retry_safe(),
        "a successor that may exist must not be asked for a second time"
    );

    // Nothing moved: the source is still the only session this connection has.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments.len(), 1);
    assert_eq!(snapshot.attachments[0].session_id.0.as_ref(), SESSION);
    assert_eq!(snapshot.attachments[0].state, AcpAttachmentState::Idle);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An agent that is not ego has no control methods, and none are attempted.
///
/// The refusal is read off the immutable snapshot taken at `initialize`, so it
/// costs no round trip and cannot change under a caller who retries. The
/// scenario proves the second half: it ends on `expect_no_frame`, which fails
/// by naming any frame these three calls should never have written.
#[tokio::test]
async fn an_agent_without_the_ego_extensions_refuses_every_control_before_the_wire() {
    let fixture = Fixture::with("no-ego-extensions");
    let connection = fixture.connect().await;
    let capabilities = connection
        .capabilities
        .as_ref()
        .expect("an initialized connection has a snapshot");
    assert_eq!(capabilities.ego_hold_version, None);
    assert_eq!(capabilities.ego_compact_version, None);

    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let session_id = session.session_id.clone();

    for error in [
        fixture
            .manager
            .pause_turn(connection.connection_id, hold(&session_id, HOLD_REQUEST))
            .await
            .expect_err("there is no pause method to call"),
        fixture
            .manager
            .resume_turn(connection.connection_id, hold(&session_id, HOLD_REQUEST))
            .await
            .expect_err("there is no resume method to call"),
        fixture
            .manager
            .compact(
                connection.connection_id,
                EgoCompactRequest {
                    session_id: session_id.clone(),
                    request_id: COMPACT_REQUEST.parse().expect("a request id"),
                },
            )
            .await
            .expect_err("there is no compact method to call"),
    ] {
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        assert!(
            !error.retryable,
            "the snapshot it was refused from does not change: {error:?}"
        );
    }

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Catches: pre-prompt offers are journaled but never made settable or published.
#[tokio::test]
async fn async_config_offer_is_published_and_settable_without_a_prompt() {
    let fixture = Fixture::with("async-config");
    let connection = fixture.connect().await;
    let first = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    assert_eq!(
        first.config_options,
        [] as [agent_client_protocol::schema::v1::SessionConfigOption; 0]
    );
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let other = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    until(&mut stream, |event| matches!(event, AcpClientEvent::SessionUpdate { update } if matches!(&**update, v1::SessionUpdate::ConfigOptionUpdate(_)))).await;
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot
            .attachments
            .iter()
            .find(|a| a.session_id == other.session_id)
            .unwrap()
            .config_options,
        [] as [agent_client_protocol::schema::v1::SessionConfigOption; 0]
    );
    let offered = snapshot
        .attachments
        .iter()
        .find(|a| a.session_id == first.session_id)
        .unwrap();
    assert_eq!(
        ids(&offered.config_options),
        ["model", "effort", "mode", "sandbox"]
    );
    assert_eq!(
        current(&offered.config_options, "mode").as_deref(),
        Some("default")
    );
    fixture
        .manager
        .set_config_option(
            connection.connection_id,
            config(&first.session_id, "model", value("ollama/gemma4:12b-mlx")),
        )
        .await
        .expect("the recorded model is forwarded");
    fixture
        .manager
        .list_sessions(connection.connection_id, v1::ListSessionsRequest::default())
        .await
        .unwrap();
    until(&mut stream, |event| matches!(event, AcpClientEvent::SessionUpdate { update } if matches!(&**update, v1::SessionUpdate::ConfigOptionUpdate(options) if options.config_options.is_empty()))).await;
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot
            .attachments
            .iter()
            .all(|a| a.config_options.is_empty())
    );
    assert!(
        snapshot
            .attachments
            .iter()
            .any(|a| a.session_id == other.session_id)
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
