//! Story 785: the JSON an ACP host actually receives.
//!
//! Every other test in this suite matches on the Rust enum, which is the one
//! reader that never sees the wire. A SolidJS store does: it gets a `Channel`
//! frame on the desktop and a WebSocket text frame in the browser, both of them
//! `serde_json` output, and it has to discriminate nine event kinds off one
//! `kind` field. So the shape of that JSON is a contract, and a contract with
//! no test is a guess.
//!
//! It also catches a failure the typed tests structurally cannot: an internally
//! tagged enum refuses at *serialization* time, not at compile time, so a
//! variant that cannot be written is indistinguishable from one nobody has sent
//! yet until something tries.

use agent_client_protocol::schema::v1;
use serde_json::{Value, json};
use tuicommander_lib::acp::{
    AcpActiveTool, AcpAttachmentState, AcpChatActivity, AcpChatActivityState, AcpChildActivity,
    AcpClientEvent, AcpConnectionId, AcpConnectionState, AcpEventEnvelope, AcpHostRequestId,
    AcpHumanWait, AcpHumanWaitKind, AcpPendingInteraction, AcpSessionAuthority, AcpStreamFrame,
};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";

/// Serialize, or fail naming the variant that could not be written.
///
/// `to_value` on an internally tagged enum whose payload is not a map returns
/// `Err`, so an `unwrap` here would report "cannot serialize tagged newtype
/// variant" with no indication of which frame produced it.
fn wire(event: AcpClientEvent) -> Value {
    let label = format!("{event:?}");
    serde_json::to_value(&event)
        .unwrap_or_else(|err| panic!("this event cannot be put on the wire at all: {label}: {err}"))
}

#[test]
fn every_event_kind_serializes_to_an_object_tagged_by_kind() {
    let cases = [
        AcpClientEvent::ConnectionState {
            state: AcpConnectionState::Ready,
        },
        AcpClientEvent::AttachmentState {
            state: AcpAttachmentState::Idle,
        },
        AcpClientEvent::TurnStarted,
        AcpClientEvent::TurnSettled {
            stop_reason: v1::StopReason::EndTurn,
            usage: None,
        },
    ];

    for case in cases {
        let label = format!("{case:?}");
        let value = wire(case);
        let object = value
            .as_object()
            .unwrap_or_else(|| panic!("{label} must be a JSON object, got {value}"));
        assert!(
            object.get("kind").and_then(Value::as_str).is_some(),
            "{label} must carry a string `kind` discriminant, got {value}"
        );
    }
}

/// The two state events put the state under `state`, not under its own name.
///
/// These are the two variants whose whole payload is one enum value, so they are
/// the two serde has nowhere to put. As bare newtype variants it derived a key
/// from the value and emitted `{"kind":"connectionState","failed":null}` — a
/// different object shape per state, which no discriminated union can type and
/// no reader can read without enumerating keys. Asserting the exact object is
/// the point: a regression here does not throw, it just stops being readable.
#[test]
fn the_state_events_carry_the_state_under_a_fixed_key() {
    assert_eq!(
        wire(AcpClientEvent::ConnectionState {
            state: AcpConnectionState::Failed,
        }),
        json!({"kind": "connectionState", "state": "failed"})
    );
    assert_eq!(
        wire(AcpClientEvent::AttachmentState {
            state: AcpAttachmentState::Prompting,
        }),
        json!({"kind": "attachmentState", "state": "prompting"})
    );
}

/// A pending interaction is camelCase all the way down, like the events are.
///
/// It is the same enum shape as `AcpClientEvent` and it carries the same three
/// field names, so it had the same defect for the same reason. It is also read
/// from a second place — the snapshot and `GET .../interactions` — which is
/// exactly the path a client uses to catch up after missing the stream.
#[test]
fn a_pending_interaction_names_its_ids_in_camel_case() {
    let value = serde_json::to_value(AcpPendingInteraction::Permission {
        request_id: AcpHostRequestId::new(),
        session_id: v1::SessionId::new(SESSION),
        request: Box::new(v1::RequestPermissionRequest::new(
            v1::SessionId::new(SESSION),
            v1::ToolCallUpdate::new(
                v1::ToolCallId::new("call-1"),
                v1::ToolCallUpdateFields::default(),
            ),
            vec![],
        )),
    })
    .expect("a pending interaction must serialize");

    assert_eq!(
        value.get("kind").and_then(Value::as_str),
        Some("permission")
    );
    assert!(value.get("requestId").is_some(), "got {value}");
    assert_eq!(value.get("sessionId"), Some(&json!(SESSION)), "got {value}");
    assert!(
        value.get("request_id").is_none() && value.get("session_id").is_none(),
        "the wire is camelCase only, got {value}"
    );
}

/// A frame is one object, and the envelope's fields sit beside `kind`.
///
/// The store reads `sequence` off the same object it reads `kind` off; if the
/// envelope were nested under a content key instead, every cursor update would
/// need a second hop and the resume path would silently never advance.
#[test]
fn an_event_frame_flattens_the_envelope_beside_its_kind() {
    let connection_id = AcpConnectionId::new();
    let frame = AcpStreamFrame::Event(Box::new(AcpEventEnvelope {
        connection_id,
        generation: 7,
        sequence: 42,
        session_id: Some(v1::SessionId::new(SESSION)),
        turn_id: None,
        event: AcpClientEvent::TurnStarted,
    }));

    let value = serde_json::to_value(&frame).expect("an event frame must serialize");
    assert_eq!(value.get("kind").and_then(Value::as_str), Some("event"));
    assert_eq!(value.get("sequence"), Some(&json!(42)));
    assert_eq!(value.get("generation"), Some(&json!(7)));
    assert_eq!(
        value.get("connectionId").and_then(Value::as_str),
        Some(connection_id.to_string().as_str())
    );
    assert_eq!(value.get("sessionId"), Some(&json!(SESSION)));
    assert_eq!(value.get("turnId"), Some(&Value::Null));
    assert!(
        value.get("event").is_some(),
        "the event itself stays under `event`, got {value}"
    );
}

/// Both interaction events name their request id in camelCase.
///
/// A store dismisses a rendered question by id when `interactionSettled`
/// arrives, possibly answered by a different client; a snake_case key here
/// would leave every question on screen forever and nothing would fail.
#[test]
fn interaction_events_name_their_request_id_in_camel_case() {
    let request_id = AcpHostRequestId::new();
    let settled = wire(AcpClientEvent::PermissionSettled {
        request_id,
        outcome: v1::RequestPermissionOutcome::Cancelled,
    });

    assert_eq!(
        settled.get("kind").and_then(Value::as_str),
        Some("permissionSettled"),
        "got {settled}"
    );
    assert_eq!(
        settled.get("requestId").and_then(Value::as_str),
        Some(request_id.to_string().as_str()),
        "got {settled}"
    );
    assert!(
        settled.get("request_id").is_none(),
        "the wire is camelCase only, got {settled}"
    );
}

// ---------------------------------------------------------------------------
// Who may hand ego an MCP server (plan §4.5)
// ---------------------------------------------------------------------------

/// A request body cannot name an MCP server.
///
/// `connect` and `reconnect` take the spawn guard because they launch a
/// process. The session routes under them do not, and they are reachable from a
/// browser — so a body that could carry `mcpServers` would let whoever sends one
/// point ego at any HTTP endpoint it liked, with any headers, on a connection
/// that is already legitimate. The guard upstream cannot see that and never
/// will.
#[test]
fn a_session_authority_refuses_a_body_that_names_an_mcp_server() {
    let refused = serde_json::from_value::<AcpSessionAuthority>(json!({
        "cwd": "/repo",
        "additionalDirectories": [],
        "mcpServers": [{
            "name": "attacker",
            "url": "https://example.invalid/mcp",
            "headers": [{"name": "authorization", "value": "Bearer stolen"}]
        }]
    }))
    .expect_err("a body naming an MCP server must be refused, not stripped");

    assert!(
        refused.to_string().contains("mcpServers"),
        "the refusal must name the field that caused it, got: {refused}"
    );
}

/// The authority a caller *may* send still deserializes, and reaches nothing.
///
/// The refusal above is only worth having if the ordinary body still works and
/// arrives with an empty server list for the server to fill.
#[test]
fn a_session_authority_without_servers_is_accepted_and_reaches_nothing_yet() {
    let authority = serde_json::from_value::<AcpSessionAuthority>(json!({
        "cwd": "/repo",
        "additionalDirectories": ["/repo/vendor"],
    }))
    .expect("the authority a caller may send must still deserialize");

    assert_eq!(authority.cwd, std::path::PathBuf::from("/repo"));
    assert_eq!(authority.additional_directories.len(), 1);
    assert!(
        authority.mcp_servers.is_empty(),
        "the server list is synthesised server-side, so it starts empty"
    );
}

/// The TUICommander entry a session gets, on the wire ego reads it from.
///
/// The ACP transport serializes to a tagged object with no command and no
/// address, and this file exists because the Rust enum is the one reader that
/// never sees the keys. What serves it is story 1156's.
#[test]
fn the_synthesised_entry_names_our_server_on_the_acp_transport() {
    let value = serde_json::to_value(tuicommander_lib::acp::tuicommander_acp_mcp_server())
        .expect("the entry must serialize");
    assert_eq!(
        value,
        json!({ "type": "acp", "name": "tuicommander", "serverId": "tuicommander" }),
        "no command, url or env: the connection is the server's address"
    );
}

/// A tool call's `kind` must not collide with the event's own `kind` tag.
///
/// `AcpClientEvent` is internally tagged on `kind` and its `SessionUpdate`
/// variant is a newtype, so the update's fields are flattened beside that tag —
/// and `ToolCall` has a field called `kind` too. Two keys of the same name in
/// one object is not an error anywhere: serde_json writes both, and every
/// reader keeps the last, so the event would arrive announcing itself as a
/// `read` rather than as a session update.
#[test]
fn a_tool_call_kind_does_not_overwrite_the_event_kind() {
    let mut call = v1::ToolCall::new(v1::ToolCallId::new("t1"), "Read AGENTS.md");
    call.kind = v1::ToolKind::Read;
    let event = AcpClientEvent::SessionUpdate {
        update: Box::new(v1::SessionUpdate::ToolCall(call)),
    };

    let text = serde_json::to_string(&event).expect("the event must serialize");
    let value: Value = serde_json::from_str(&text).expect("and must parse back");

    assert_eq!(
        value.get("kind").and_then(Value::as_str),
        Some("sessionUpdate"),
        "the event lost its own tag to the tool call's: {text}"
    );
}

/// Story 2637: the activity event carries the whole activity in camel case.
///
/// The frontend keeps whichever of the snapshot and the stream has the higher
/// `revision`, so the field names here are what a reconnect compares on.
#[test]
fn an_activity_change_carries_the_whole_activity_in_camel_case() {
    let request_id = AcpHostRequestId::new();
    let activity = AcpChatActivity {
        state: AcpChatActivityState::Awaiting,
        human_waits: vec![AcpHumanWait {
            request_id,
            kind: AcpHumanWaitKind::Question,
        }],
        tools: vec![AcpActiveTool {
            tool_call_id: "call-1".into(),
            title: "Write a file".into(),
        }],
        children: vec![AcpChildActivity {
            turn_id: "t".into(),
            item_id: "i".into(),
            child: json!({"kind": "foreground", "turn": "f"}),
        }],
        interrupted: false,
        revision: 7,
    };
    assert_eq!(
        wire(AcpClientEvent::ActivityChanged { activity }),
        json!({
            "kind": "activityChanged",
            "activity": {
                "state": "awaiting",
                "humanWaits": [{"requestId": request_id.to_string(), "kind": "question"}],
                "tools": [{"toolCallId": "call-1", "title": "Write a file"}],
                "children": [{"turnId": "t", "itemId": "i", "child": {"kind": "foreground", "turn": "f"}}],
                "interrupted": false,
                "revision": 7
            }
        })
    );
}
