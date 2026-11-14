//! Story092 batch 1: final ACP launch, initialize, and capability contract.
//!
//! These tests intentionally name the crate-visible ACP boundary that the
//! implementation must provide.  They do not exercise later session or turn
//! lifecycle behavior.

use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde_json::json;
use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpConnectRequest, AcpOperation, AcpReconnectRequest, AcpUnavailableReason,
    EgoAcpConfig, build_initialize_request, capability_snapshot, launch_spec,
};

use crate::acp_support;

#[test]
fn configured_ego_launch_is_direct_and_has_exact_argv() {
    let executable = acp_support::absolute("/opt/ego/bin/ego");
    let root = acp_support::absolute("/private/tmp/worktree");
    let spec = launch_spec(
        &EgoAcpConfig {
            executable: executable.clone(),
            profile: String::new(),
            master_prompt: String::new(),
            ai_chat: false,
        },
        &root,
    )
    .unwrap();

    assert_eq!(spec.program, executable);
    assert_eq!(
        spec.args,
        ["acp", "-C", root.to_str().expect("a UTF-8 root")]
    );
}

#[test]
fn selected_user_profile_is_one_launch_argument() {
    let root = acp_support::absolute("/private/tmp/worktree");
    let spec = launch_spec(
        &EgoAcpConfig {
            executable: acp_support::absolute("/opt/ego/bin/ego"),
            profile: "coordinator".to_string(),
            master_prompt: String::new(),
            ai_chat: false,
        },
        &root,
    )
    .unwrap();

    assert_eq!(
        spec.args,
        [
            "acp",
            "-C",
            root.to_str().unwrap(),
            "--profile",
            "coordinator"
        ]
    );
}

#[test]
fn ambiguous_profile_names_are_refused_before_launch() {
    for profile in [
        "-other",
        "my profile",
        " profile",
        "profile\nother",
        "name\u{a0}part",
    ] {
        let error = launch_spec(
            &EgoAcpConfig {
                executable: acp_support::absolute("/opt/ego/bin/ego"),
                profile: profile.to_string(),
                master_prompt: String::new(),
                ai_chat: false,
            },
            &acp_support::absolute("/private/tmp/worktree"),
        )
        .expect_err("ambiguous profile must not reach ego");
        assert!(error.message.contains("profile"), "{error:?}");
    }
    let too_long = "a".repeat(65);
    assert!(
        launch_spec(
            &EgoAcpConfig {
                executable: acp_support::absolute("/opt/ego/bin/ego"),
                profile: too_long,
                master_prompt: String::new(),
                ai_chat: false,
            },
            &acp_support::absolute("/private/tmp/worktree"),
        )
        .is_err()
    );
}

#[tokio::test]
async fn selected_profile_reaches_the_process_and_session_new_carries_no_host_policy() {
    let fixture = acp_support::Fixture::with("session-new");
    let root = fixture.root();
    std::fs::write(root.join("expected-profile.txt"), "coordinator").unwrap();
    let mut config = acp_support::Fixture::config();
    config.profile = "coordinator".to_string();

    let connection = fixture
        .manager
        .connect(
            &config,
            AcpConnectRequest {
                root: root.clone(),
                env: None,
            },
        )
        .await
        .expect("ego received the selected profile");
    fixture
        .manager
        .new_session(connection.connection_id, acp_support::authority(root))
        .await
        .expect("session/new carries no host policy");
    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect("complete the fixture scenario");
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_profile_on_reconnect_preserves_the_current_connection() {
    let fixture = acp_support::Fixture::with("ready");
    let connection = fixture.connect().await;
    let mut config = acp_support::Fixture::config();
    config.profile = "-invalid".to_string();

    let error = fixture
        .manager
        .reconnect(
            &config,
            AcpReconnectRequest {
                connection_id: connection.connection_id,
                root: fixture.root(),
            },
        )
        .await
        .expect_err("an invalid profile cannot replace the live ego process");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);
    assert_eq!(
        fixture.manager.snapshot(connection.connection_id).unwrap(),
        connection
    );
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[test]
fn initialize_is_v1_and_advertises_exact_client_capabilities() {
    let request = build_initialize_request();
    assert_eq!(request.protocol_version, ProtocolVersion::V1);
    assert_eq!(
        serde_json::to_value(&request.client_capabilities).unwrap(),
        json!({
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
            // Advertised because this client seats an `elicitation/create` and
            // lets a person answer it. Ego reads exactly this key and settles
            // its questions `Unavailable` without it, so the claim here and the
            // seat in the actor have to be made and unmade together.
            "elicitation": {"form": {}},
            // The opt-ins to ego's child activity frames (ego story 772-d55a)
            // and compacted updates (#2815). Ego sends each only to a client
            // that names this exact version.
            "_meta": {"ego": {"childActivity": {"version": 1}, "compacted": {"version": 1}}}
        })
    );
}

/// The bytes ego actually sends, not the shape this plan predicted it would.
///
/// Two things were guessed wrong and both made the extension undiscoverable.
/// The metadata hangs off `agentCapabilities`, not off the response, because it
/// *is* a capability; and pause and resume arrive as one `hold` entry carrying
/// both method names under one version, not as two independently versioned
/// entries. The second is the better contract and the reason to follow it
/// rather than ask ego to change: they are two halves of one extension, and two
/// versions can disagree — a `pause` at 1 with a `resume` at 2 is a state no
/// agent can be in and every client would then have to decide what to do about.
///
/// Verified against `ego`'s `crates/ego-acp/src/serve.rs` capability builder,
/// its `host_hold_091_red.rs` assertions and `docs/06-acp.md` §4.
fn ego_initialize_response() -> serde_json::Value {
    json!({
        "protocolVersion": 1,
        "agentInfo": {"name": "ego", "title": "Ego", "version": "1"},
        "agentCapabilities": {
            "loadSession": true,
            "sessionCapabilities": {"list": {}, "delete": {}, "additionalDirectories": {}, "fork": {}, "resume": {}, "close": {}},
            "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
            "mcpCapabilities": {"http": true, "sse": true, "acp": true},
            "_meta": {"ego": {
                "hold": {"version": 1, "pause": "_ego/pause", "resume": "_ego/resume"},
                "compact": {"version": 1, "method": "_ego/compact"}
            }}
        }
    })
}

#[test]
fn initialize_response_becomes_an_immutable_full_capability_snapshot() {
    let response: v1::InitializeResponse =
        serde_json::from_value(ego_initialize_response()).unwrap();
    let snapshot = capability_snapshot(&response).unwrap();
    assert!(snapshot.load && snapshot.list && snapshot.resume && snapshot.fork);
    assert!(snapshot.delete && snapshot.close && snapshot.prompt_image);
    assert!(snapshot.prompt_embedded_context && snapshot.mcp_http && snapshot.mcp_sse);
    // ACP is read from the recorded response; stdio is a fact about this side:
    // no stdio bridge is carried any more (#796-7fa3 retired), so stdio is
    // excluded by contract.
    assert!(snapshot.mcp_acp && !snapshot.mcp_stdio);
    assert!(!snapshot.client_boolean_config);
    assert_eq!(snapshot.availability(AcpOperation::McpAcp).reason, None);
    assert_eq!(
        snapshot.availability(AcpOperation::McpStdio).reason,
        Some(AcpUnavailableReason::ExcludedByContract)
    );
    // Form elicitation reads as available because this client now seats an
    // `elicitation/create` and lets a person answer it. It is a fact about
    // this side, so it does not move with the recorded response above.
    assert!(snapshot.client_form_elicitation);
    assert!(
        snapshot
            .availability(AcpOperation::ClientFormElicitation)
            .available
    );
    assert_eq!(
        snapshot
            .availability(AcpOperation::ClientBooleanConfig)
            .reason,
        Some(AcpUnavailableReason::ExcludedByContract)
    );
    assert_eq!(snapshot.ego_hold_version, Some(1));
    assert_eq!(snapshot.ego_compact_version, Some(1));
    assert!(snapshot.availability(AcpOperation::Pause).available);
    assert!(snapshot.availability(AcpOperation::ResumeTurn).available);
    assert_eq!(snapshot.protocol, ProtocolVersion::V1);
}

#[test]
fn absent_or_mismatched_capabilities_are_rejected_before_write() {
    let response: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1,
        "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&response).unwrap();

    assert_eq!(
        snapshot.availability(AcpOperation::Load).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::Compact).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );

    let mismatched: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1, "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {"_meta": {"ego": {
            "hold": {"version": 2, "pause": "_ego/pause", "resume": "_ego/resume"},
            "compact": {"version": 1, "method": "_ego/compact"}
        }}}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&mismatched).unwrap();
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::ResumeTurn).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert!(snapshot.availability(AcpOperation::Compact).available);

    // A version this client knows, spelled with a method name it does not.
    // Guessing that `_ego/hold` is `_ego/pause` under another name is exactly
    // the guess the snapshot exists to refuse.
    let renamed: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1, "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {"_meta": {"ego": {
            "hold": {"version": 1, "pause": "_ego/hold", "resume": "_ego/resume"}
        }}}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&renamed).unwrap();
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert_eq!(snapshot.ego_hold_version, None);
}

/// The pinned client and the pinned agent agree about the extension.
///
/// The unit test above uses a hand-written response; this one takes the bytes
/// off the wire from the fixture agent, whose `ready` scenario is the recorded
/// ego shape. Without it, both ends of the check would be this repository's own
/// opinion of what ego sends — which is precisely how the shape went wrong.
#[tokio::test]
async fn the_agent_this_client_launches_advertises_the_hold_extension() {
    let fixture = acp_support::Fixture::with("ready");
    let snapshot = fixture.connect().await;
    let capabilities = snapshot
        .capabilities
        .expect("a ready connection is negotiated");

    assert_eq!(capabilities.ego_hold_version, Some(1));
    assert_eq!(capabilities.ego_compact_version, Some(1));
    assert!(capabilities.availability(AcpOperation::Pause).available);
    assert!(
        capabilities
            .availability(AcpOperation::ResumeTurn)
            .available
    );
    assert!(capabilities.availability(AcpOperation::Compact).available);
    assert!(capabilities.load && capabilities.list && capabilities.fork);

    fixture
        .manager
        .disconnect(snapshot.connection_id)
        .await
        .unwrap();
}
