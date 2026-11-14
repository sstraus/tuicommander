//! Replay real isolated-daemon exchanges; see the fixture's provenance file.
use super::*;

fn capture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/managed-reconnect-2008.json")).unwrap()
}

fn exchange(label: &str) -> serde_json::Value {
    capture()["exchanges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["label"] == label)
        .unwrap_or_else(|| panic!("missing captured exchange {label}"))
        .clone()
}

fn payload(response: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

async fn replay(
    state: Arc<AppState>,
    label: &str,
    protocol_session: Option<&str>,
) -> (serde_json::Value, Option<String>) {
    let row = exchange(label);
    let mut headers = HeaderMap::new();
    for (key, value) in row["headers"].as_object().unwrap() {
        headers.insert(
            axum::http::HeaderName::from_bytes(key.as_bytes()).unwrap(),
            value.as_str().unwrap().parse().unwrap(),
        );
    }
    if let Some(sid) = protocol_session {
        headers.insert(MCP_SESSION_HEADER, sid.parse().unwrap());
    }
    let response = mcp_post(
        State(state),
        ConnectInfo(loopback_addr()),
        None,
        headers,
        Json(row["request"].clone()),
    )
    .await
    .into_response();
    assert_eq!(
        response.status().as_u16(),
        row["status"].as_u64().unwrap() as u16,
        "{label}"
    );
    let sid = response
        .headers()
        .get(MCP_SESSION_HEADER)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (serde_json::from_slice(&bytes).unwrap(), sid)
}

fn state_at(path: &std::path::Path) -> Arc<AppState> {
    Arc::new(AppState::new(
        path.into(),
        path.join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(Mutex::new(crate::app_logger::LogRingBuffer::new(100))),
    ))
}

fn restore_binding(state: &AppState, peer: &str, pty: &str, cwd: &str) {
    crate::state::tests_support::insert_dummy_session(state, pty);
    crate::state::tests_support::set_session_cwd(state, pty, cwd);
    state.bind_live_pty(peer, pty);
}

// Catches: fresh process state loses the exact host identity, reads another PTY's
// mailbox, or requires a legacy initialize/register repair for modern calls.
#[tokio::test]
async fn managed_reconnect_fixture_restart_preserves_stable_identity_and_subscription() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().into());
    let peer = exchange("create-tab")["response"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let pty = exchange("resume-tab")["response"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(
        peer, pty,
        "capture must exercise a new PTY for the retained tab"
    );

    let cwd = root.path().to_str().unwrap();
    let state = state_at(root.path());
    restore_binding(&state, &peer, &peer, cwd);
    // subscribe is an orchestrator action; the captured peer is the coordinator.
    state.orchestrator_peers.insert(peer.clone());
    for label in [
        "ego-discover-before",
        "managed-inbox-before",
        "subscribe-before-restart",
    ] {
        let (response, sid) = replay(state.clone(), label, None).await;
        assert!(
            sid.is_none(),
            "modern requests must not acquire protocol sessions"
        );
        assert!(response.get("error").is_none(), "{label}: {response}");
        assert_ne!(response["result"]["isError"], true, "{label}: {response}");
    }
    let (sent, _) = replay(state.clone(), "send-retained-mail", None).await;
    let sent = payload(&sent);
    let message_id = sent["message_id"].as_str().unwrap().to_string();
    drop(state);
    // Unlike clearing routing maps, this drops the actual journal connection
    // and reconstructs every process-owned map against the same on-disk state.
    let state = state_at(root.path());
    assert!(state.mcp.to_session.is_empty());
    restore_binding(&state, &peer, &pty, cwd);
    state.orchestrator_peers.insert(peer.clone());
    let (discovery, sid) = replay(state.clone(), "ego-discover-after", None).await;
    assert!(discovery.get("error").is_none(), "{discovery}");
    assert!(sid.is_none());
    let (inbox, sid) = replay(state.clone(), "managed-inbox-after-restart", None).await;
    assert!(sid.is_none());
    assert_eq!(inbox["result"]["isError"], false, "{inbox}");
    let inbox = payload(&inbox);
    let recorded = payload(&exchange("managed-inbox-after-restart")["response"]);
    assert_eq!(inbox["messages"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["messages"][0]["id"], message_id);
    assert_eq!(
        inbox["messages"][0]["content"],
        recorded["messages"][0]["content"]
    );
    assert_eq!(inbox["messages"][0]["from_tuic_session"], peer);
    let routes = state.mcp.session_to_mcp.get(&peer).unwrap();
    assert_eq!(
        resolve_mcp_origin_repo_path(&state, Some(&routes[0])).as_deref(),
        Some(cwd)
    );
    assert_eq!(
        state.live_pty_for_peer(&peer).as_deref(),
        Some(pty.as_str())
    );
}

// Catches: a headerless external caller inherits the managed peer's project or
// receives a retryable identity failure that can restart the consumer loop.
#[tokio::test]
async fn managed_reconnect_fixture_external_context_refusals_match_live_contract() {
    let state = test_state();
    for label in ["external-headerless-inbox", "external-missing-project"] {
        let (response, sid) = replay(state.clone(), label, None).await;
        assert!(sid.is_none());
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert_eq!(payload(&response), payload(&exchange(label)["response"]));
    }
    assert!(state.peer_agents.is_empty());
    assert!(state.mcp.to_session.is_empty());
}

// Catches: reconnect recovery silently migrates a live owner's mail or drops
// the explicit migration/stranded-mail receipts promised by 554-9df0.
#[tokio::test]
async fn managed_reconnect_fixture_replacement_keeps_migration_and_live_owner_receipts() {
    let state = test_state();
    let peer = exchange("create-tab")["response"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let pty = exchange("resume-tab")["response"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    restore_binding(&state, &peer, &pty, "/fixture-project");
    replay(state.clone(), "managed-inbox-before", None).await;
    let (_, old_sid) = replay(state.clone(), "legacy-initialize-old", None).await;
    replay(state.clone(), "legacy-register-old", old_sid.as_deref()).await;
    let (sent, _) = replay(state.clone(), "send-orphan-mail", None).await;
    let orphan_id = payload(&sent)["message_id"].clone();
    let (_, new_sid) = replay(state.clone(), "legacy-initialize-replacement", None).await;
    let (response, _) = replay(state.clone(), "legacy-replace-orphan", new_sid.as_deref()).await;
    let migrated = payload(&response);
    let recorded = payload(&exchange("legacy-replace-orphan")["response"]);
    assert_eq!(migrated["mail_migrated"], recorded["mail_migrated"]);
    assert_eq!(
        migrated["superseded_identity"],
        recorded["superseded_identity"]
    );
    let (inbox, _) = replay(state.clone(), "legacy-migrated-inbox", new_sid.as_deref()).await;
    assert_eq!(payload(&inbox)["messages"][0]["id"], orphan_id);
    let (sent, _) = replay(state.clone(), "send-live-owner-mail", None).await;
    let live_id = payload(&sent)["message_id"].clone();
    let (_, claimant_sid) = replay(state.clone(), "legacy-initialize-claimant", None).await;
    let (response, _) = replay(
        state.clone(),
        "legacy-replace-live-owner",
        claimant_sid.as_deref(),
    )
    .await;
    let stranded = payload(&response);
    let recorded = payload(&exchange("legacy-replace-live-owner")["response"]);
    for key in [
        "mail_migrated",
        "mail_stranded",
        "superseded_identity",
        "identity_warning",
    ] {
        assert_eq!(stranded[key], recorded[key], "{key}: {stranded}");
    }
    let (inbox, _) = replay(state.clone(), "live-owner-retains-mail", None).await;
    assert!(
        payload(&inbox)["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == live_id)
    );
    assert_eq!(
        state.live_pty_for_peer(&peer).as_deref(),
        Some(pty.as_str())
    );
}
