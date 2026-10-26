use super::*;

const PARENT: &str = "550e8400-e29b-41d4-a716-446655440b01";
fn state() -> Arc<AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    super::super::mcp_transport_session_agent::handle_agent_unified(
        &state,
        addr(),
        &json!({
            "action":"register", "tuic_session": PARENT, "name":"parent"
        }),
        Some("launch-tests"),
    );
    state
}
fn addr() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn request() -> Value {
    json!({"idempotency_key":"retry", "parent_session_id":PARENT,
        "name":"stable-child", "keep_open":true,
        "workspace":{"existing_path":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()},
        "spawn":{"binary_path":"/bin/sh", "prompt":"perform task", "args":["-c", "exec cat", "{prompt}"]}})
}
#[cfg(unix)]
fn close(state: &Arc<AppState>, id: &str) {
    super::super::mcp_transport_session_agent::handle_session(
        state,
        &json!({"action":"kill", "session_id":id}),
        None,
    );
}
#[cfg(unix)]
fn repo(root: &std::path::Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@test.com"],
        vec!["config", "user.name", "Test"],
        vec!["commit", "--allow-empty", "-m", "test fixture"],
    ] {
        crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    }
    repo
}

// Catches: retry after workspace checkpoint allocates another checkout or starts in the parent.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_retries_after_worktree_checkpoint_without_fallback() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let repo = repo(root.path());
    let state = state();
    let mut request = request();
    request["workspace"] = json!({"repo_path":repo, "branch":"feat/launch-fixture"});
    assert!(
        execute_inner(&state, addr(), &request, Fault::AfterWorktree)
            .await
            .unwrap_err()
            .contains("injected")
    );
    assert!(state.session_maps.sessions.is_empty());
    let worktrees =
        tuic_git::worktree::get_worktree_paths(repo.to_string_lossy().into_owned()).unwrap();
    let selected = worktrees
        .values()
        .find(|workspace| workspace.branch == "feat/launch-fixture")
        .unwrap();
    let result = execute(&state, addr(), &request).await;
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(result["workspace_path"], selected.path);
    assert_ne!(result["workspace_path"], repo.to_string_lossy().as_ref());
    assert!(result["warm_artifacts"]["status"].is_string());
    assert_eq!(state.session_maps.sessions.len(), 1);
    close(&state, result["session_id"].as_str().unwrap());
}

// Catches: a child exists but the receipt checkpoint was lost, so retry spawns twice.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_recovers_child_before_receipt_checkpoint() {
    let state = state();
    let request = request();
    assert!(
        execute_inner(&state, addr(), &request, Fault::AfterChild)
            .await
            .unwrap_err()
            .contains("injected")
    );
    let child = state
        .session_maps
        .sessions
        .iter()
        .next()
        .unwrap()
        .key()
        .clone();
    let result = execute(&state, addr(), &request).await;
    assert_eq!(result["session_id"], child);
    assert_eq!(result["status"], "prompt_delivered");
    assert_eq!(
        result["prompt_delivery"], "delivered",
        "the retained PTY brief receipt proves argv handoff"
    );
    assert_eq!(state.session_maps.sessions.len(), 1);
    close(&state, &child);
}

// Catches: delivery timeout is labelled ready/delivered, or retry types the initial prompt again.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_delivery_failure_keeps_one_queued_prompt() {
    let state = state();
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let mut request = request();
    let binary = crate::test_support::fake_ssh_script(
        "launch-delivery-failure",
        "if [ \"$1\" = '--help' ]; then printf '%s\\n' '--no-alt-screen'; else read unused; fi",
        "if \"%1\"==\"--help\" (echo --no-alt-screen) else (set /p HOLD=)",
    );
    request["spawn"]["agent_type"] = json!("codex");
    request["spawn"]["binary_path"] = json!(binary);
    request["spawn"]["args"] = json!(["--profile", "fixture"]);
    let result = execute_inner(&state, addr(), &request, Fault::DuringDelivery)
        .await
        .unwrap();
    assert_eq!(result["status"], "blocked");
    assert_eq!(result["prompt_delivery"], "queued");
    assert_eq!(result["ready"], false);
    let child = result["session_id"].as_str().unwrap();
    let before = state.pending_injections.get(child).unwrap().len();
    let again = execute(&state, addr(), &request).await;
    assert_eq!(again["launch_id"], result["launch_id"]);
    assert_eq!(state.pending_injections.get(child).unwrap().len(), before);
    assert_eq!(before, 1);
    close(&state, child);
}

// Catches: a parentless run never arms the deferred initial-prompt watchdog.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_owned_prompt_timeout_reports_blocked_without_requeue() {
    let state = state();
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let binary = crate::test_support::fake_ssh_script(
        "launch-owned-prompt-timeout",
        "if [ \"$1\" = '--help' ]; then printf '%s\\n' '--no-alt-screen'; else read unused; fi",
        "if \"%1\"==\"--help\" (echo --no-alt-screen) else (set /p HOLD=)",
    );
    let mut request = request();
    request["parent_session_id"] = json!("automation:watchdog-regression");
    request["spawn"]["agent_type"] = json!("codex");
    request["spawn"]["binary_path"] = json!(binary);
    request["spawn"]["args"] = json!(["--profile", "fixture"]);
    let first = execute_owned(&state, addr(), &request).await;
    assert_eq!(first["status"], "created", "{first}");
    assert_eq!(first["prompt_delivery"], "queued");
    let child = first["session_id"].as_str().unwrap();
    assert!(!state.session_maps.session_parent.contains_key(child));
    tokio::time::pause();
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    tokio::task::yield_now().await;
    let after = execute_owned(&state, addr(), &request).await;
    let queued = state.pending_injections.get(child).unwrap().len();
    let children = state.session_maps.sessions.len();
    close(&state, child);
    assert_eq!(after["status"], "blocked", "{after}");
    assert_eq!(after["prompt_delivery"], "queued");
    assert_eq!(after["ready"], false);
    assert_eq!(
        after["reason"],
        "initial prompt delivery timed out; prompt remains queued"
    );
    assert_eq!(after["session_id"], first["session_id"]);
    assert_eq!(queued, 1);
    assert_eq!(children, 1);
}

// Catches: conflicting retry key silently changes model, name, hold policy or checkout.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_conflicting_key_never_spawns_or_falls_back() {
    let state = state();
    let request = request();
    let first = execute(&state, addr(), &request).await;
    assert!(first.get("error").is_none(), "{first}");
    for field in ["name", "keep_open", "workspace", "spawn"] {
        let mut conflict = request.clone();
        conflict[field] = match field {
            "keep_open" => json!(false),
            "workspace" => json!({"existing_path":"/missing"}),
            "spawn" => json!({"prompt":"another task"}),
            _ => json!("different-child"),
        };
        let result = execute(&state, addr(), &conflict).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("idempotency_conflict"),
            "{result}"
        );
    }
    assert_eq!(state.session_maps.sessions.len(), 1);
    close(&state, first["session_id"].as_str().unwrap());
}

// Catches: deterministic binary refusal falls back to a default agent/checkout on retry.
#[tokio::test]
async fn launch_transaction_deterministic_refusal_is_retained() {
    let state = state();
    let mut request = request();
    request["spawn"]["binary_path"] = json!("not-absolute");
    let first = execute(&state, addr(), &request).await;
    assert_eq!(first["status"], "blocked");
    assert!(first["reason"].as_str().unwrap().contains("absolute"));
    let again = execute(&state, addr(), &request).await;
    assert_eq!(first, again);
    assert!(state.session_maps.sessions.is_empty());
    let read = execute(&state, addr(), &json!({"launch_id":first["launch_id"]})).await;
    assert_eq!(read, first);
}

// Catches: journal restart drops key tombstones and creates a replacement child.
#[tokio::test]
async fn launch_transaction_interrupted_side_effect_blocks_without_retry() {
    let state = state();
    let request = request();
    for stage in ["worktree_started", "spawn_started"] {
        let mut request = request.clone();
        request["idempotency_key"] = json!(stage);
        let parsed: Request = serde_json::from_value(request.clone()).unwrap();
        let key = hash(&serde_json::to_vec(&("peer", PARENT, stage)).unwrap());
        let record = Record {
            launch_id: uuid::Uuid::new_v4().to_string(),
            digest: hash(
                &serde_json::to_vec(&canonical(serde_json::to_value(parsed).unwrap())).unwrap(),
            ),
            session_id: uuid::Uuid::new_v4().to_string(),
            stage: stage.into(),
            workspace_path: None,
            child: None,
            prompt_handoff: false,
            reason: None,
            revision: 0,
        };
        let journal = Journal(state.data_dir.join("managed-launches.sqlite"));
        journal.reserve(&key, record.clone()).unwrap();
        drop(journal);
        let result = execute(&state, addr(), &request).await;
        assert_eq!(result["status"], "blocked");
        assert_eq!(result["session_id"], record.session_id);
        assert_eq!(result["launch_id"], record.launch_id);
        assert!(state.session_maps.sessions.is_empty());
    }
}

// Catches: overlapping public retries both gain creation authority and spawn children.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_overlapping_requests_create_one_child() {
    let state = state();
    let request = request();
    let (first, second) = tokio::join!(
        execute(&state, addr(), &request),
        execute(&state, addr(), &request),
    );
    assert!(first.get("error").is_none(), "{first}");
    assert!(second.get("error").is_none(), "{second}");
    let child = first["session_id"].as_str().unwrap();
    let children = state.session_maps.sessions.len();
    let tasks = state.tasks.live_ids_for_session(child);
    close(&state, child);
    assert_eq!(first["launch_id"], second["launch_id"]);
    assert_eq!(first["session_id"], second["session_id"]);
    assert_eq!(first["child"]["task_id"], second["child"]["task_id"]);
    assert_eq!(children, 1, "only one public request may create a PTY");
    assert_eq!(tasks.len(), 1, "only one spawn may acquire a task handle");
}

// Catches: output or a generic idle shell is treated as native agent readiness.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_output_alone_never_proves_ready() {
    let state = state();
    let result = execute(&state, addr(), &request()).await;
    assert_eq!(result["status"], "prompt_delivered");
    assert_eq!(result["ready"], false);
    close(&state, result["session_id"].as_str().unwrap());
}

// Catches: keyed retries deliver the task to two children, as unkeyed spawn retries do.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_retry_measurement_reduces_two_children_to_one() {
    let state = state();
    let request = request();
    let mut legacy = request["spawn"].clone();
    legacy["action"] = json!("spawn");
    legacy["cwd"] = request["workspace"]["existing_path"].clone();
    let mut old_children = Vec::new();
    for _ in 0..2 {
        let result = super::super::mcp_transport_session_agent::handle_agent_with_parent_cwd(
            &state,
            addr(),
            &legacy,
            Some("launch-tests"),
            None,
        );
        assert!(result.get("error").is_none(), "{result}");
        old_children.push(result["session_id"].as_str().unwrap().to_owned());
    }
    assert_eq!(state.session_maps.sessions.len(), 2);
    assert_ne!(old_children[0], old_children[1]);
    for child in old_children {
        close(&state, &child);
    }
    assert!(state.session_maps.sessions.is_empty());
    let first = execute(&state, addr(), &request).await;
    let again = execute(&state, addr(), &request).await;
    assert_eq!(first["session_id"], again["session_id"]);
    assert_eq!(state.session_maps.sessions.len(), 1);
    close(&state, first["session_id"].as_str().unwrap());
}

// Catches: recovery fabricates delivery when both the journal result and PTY brief receipt are absent.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_missing_brief_receipt_stays_unknown() {
    let state = state();
    let request = request();
    assert!(
        execute_inner(&state, addr(), &request, Fault::AfterChild)
            .await
            .is_err()
    );
    let child = state
        .session_maps
        .sessions
        .iter()
        .next()
        .unwrap()
        .key()
        .clone();
    // Inject loss of the optional PTY receipt at the same pre-checkpoint boundary.
    state
        .session_maps
        .sessions
        .get(&child)
        .unwrap()
        .lock()
        .launch_receipt = None;
    let result = execute(&state, addr(), &request).await;
    assert_eq!(result["session_id"], child);
    assert_eq!(result["prompt_delivery"], "unknown");
    assert_eq!(result["status"], "created");
    assert_eq!(result["ready"], false);
    assert_eq!(state.session_maps.sessions.len(), 1);
    close(&state, &child);
}

// Catches: a worktree exists but its checkpoint was lost, so retry allocates/falls back.
#[cfg(unix)]
#[tokio::test]
async fn launch_transaction_partial_worktree_creation_never_retries_or_uses_parent() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let repo = repo(root.path());
    let state = state();
    let mut request = request();
    request["workspace"] = json!({"repo_path":repo, "branch":"feat/partial-launch-fixture"});
    assert!(
        execute_inner(&state, addr(), &request, Fault::AfterWorktreeEffect)
            .await
            .unwrap_err()
            .contains("injected")
    );
    let first_paths =
        tuic_git::worktree::get_worktree_paths(repo.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        first_paths
            .values()
            .filter(|entry| entry.branch == "feat/partial-launch-fixture")
            .count(),
        1
    );
    let result = execute(&state, addr(), &request).await;
    assert_eq!(result["status"], "blocked");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("worktree_creation_uncertain")
    );
    assert!(state.session_maps.sessions.is_empty());
    assert_eq!(
        tuic_git::worktree::get_worktree_paths(repo.to_string_lossy().into_owned())
            .unwrap()
            .len(),
        first_paths.len()
    );
}

// Catches: HTTP starts another transaction or bypasses the shared refusal contract.
#[tokio::test]
async fn launch_transaction_http_and_mcp_read_the_same_durable_refusal() {
    let state = state();
    let mut request = request();
    request["spawn"]["binary_path"] = json!("not-absolute");
    let Json(first) = http(
        State(state.clone()),
        ConnectInfo(addr()),
        Json(request.clone()),
    )
    .await;
    request["action"] = json!("launch");
    let mcp = super::super::mcp_transport::handle_mcp_tool_call_with_context(
        &state,
        addr(),
        "agent",
        &request,
        Some("launch-tests"),
        None,
    )
    .await;
    assert_eq!(first, mcp);
    assert_eq!(first["status"], "blocked");
    let Json(read) = http(
        State(state.clone()),
        ConnectInfo(addr()),
        Json(json!({"launch_id":first["launch_id"]})),
    )
    .await;
    assert_eq!(first, read);
    assert!(state.session_maps.sessions.is_empty());
}

// Catches: remote callers create a durable reservation before the localhost refusal.
#[tokio::test]
async fn launch_transaction_remote_refusal_has_no_side_effects() {
    let state = state();
    let result = execute(&state, "192.168.1.2:1".parse().unwrap(), &request()).await;
    assert!(result["error"].as_str().unwrap().contains("localhost"));
    assert!(!state.data_dir.join("managed-launches.sqlite").exists());
    assert!(state.session_maps.sessions.is_empty());
}

// Catches: automation and MCP child creation diverge on stable naming/keep-open, or automation retries spawn twice.
#[cfg(unix)]
#[tokio::test]
async fn automation_and_mcp_launch_share_name_hold_and_retry_policy() {
    use crate::automations::dispatcher::{DispatchEffects, LaunchRequest, native::NativeEffects};
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let repo = repo(root.path()).canonicalize().unwrap();
    let config = serde_json::from_value(json!({"launch_instructions":"", "agents":{"gemini":{
        "run_configs":[{"name":"fixture-profile", "command":"/bin/sh", "args":["-c", "exec cat", "{prompt}"]}]
    }}})).unwrap();
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = state();
    let effects = NativeEffects(state.clone());
    let automation = LaunchRequest {
        run_id: "same-run".into(),
        name: "fixture".into(),
        run_config: "fixture-profile".into(),
        workspace: repo.to_string_lossy().into_owned(),
        prompt: "perform task".into(),
    };
    let automated = effects.launch(automation.clone()).await.unwrap();
    let mut request = request();
    request["name"] = json!("Automation fixture (same-run)");
    request["workspace"] = json!({"existing_path":repo});
    request["spawn"] = json!({"agent_type":"fixture-profile", "prompt":"perform task"});
    request["action"] = json!("launch");
    let mcp = super::super::mcp_transport::handle_mcp_tool_call_with_context(
        &state,
        addr(),
        "agent",
        &request,
        Some("launch-tests"),
        None,
    )
    .await;
    let child = mcp["session_id"].as_str().unwrap();
    let auto_name = state
        .session_maps
        .sessions
        .get(&automated.session_id)
        .unwrap()
        .lock()
        .display_name
        .clone();
    let mcp_name = state
        .session_maps
        .sessions
        .get(child)
        .unwrap()
        .lock()
        .display_name
        .clone();
    assert_eq!(auto_name, mcp_name);
    assert!(state.keep_open_sessions.contains(child));
    assert!(
        state.keep_open_sessions.contains(&automated.session_id),
        "automation must use the same explicit hold policy as MCP"
    );
    let retry = effects.launch(automation).await.unwrap();
    assert_eq!(automated.session_id, retry.session_id);
    assert_eq!(
        state.session_maps.sessions.len(),
        2,
        "one automation child and one separately keyed MCP child"
    );
    close(&state, &automated.session_id);
    close(&state, child);
}
