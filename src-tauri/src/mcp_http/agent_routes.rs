use crate::pty::spawn_reader_thread;
use crate::{AppState, MAX_CONCURRENT_SESSIONS, PtySession};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(feature = "desktop")]
use tauri::Emitter;
use uuid::Uuid;

use super::guards::{Authenticated, require_local_or_auth};
use super::types::*;

pub(super) async fn boss_attention(State(state): State<Arc<AppState>>) -> Response {
    match tokio::task::spawn_blocking(move || crate::boss_attention::snapshot(&state)).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OutsideRequestAnswer {
    request_id: String,
    decision: String,
}

pub(super) async fn answer_outside_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OutsideRequestAnswer>,
) -> Response {
    match crate::fabbrica_attention::answer_outside_request(
        &state,
        &body.request_id,
        &body.decision,
    )
    .await
    {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    }
}

pub(super) async fn detect_agents() -> impl IntoResponse {
    let mut results = Vec::new();
    for name in crate::agent::KNOWN_AGENT_BINARIES {
        let detection = crate::agent::detect_agent_binary(name.to_string()).await;
        results.push(serde_json::json!({
            "name": name,
            "path": detection.path,
            "version": detection.version,
            "supports_no_alt_screen": detection.supports_no_alt_screen,
        }));
    }
    Json(results)
}

pub(super) async fn detect_agent_binary_http(Query(q): Query<DetectBinaryQuery>) -> Response {
    if !crate::agent::KNOWN_AGENT_BINARIES.contains(&q.binary.as_str()) {
        return Json(serde_json::json!({"error": "Unknown agent"})).into_response();
    }
    let detection = crate::agent::detect_agent_binary(q.binary).await;
    Json(serde_json::json!({
        "path": detection.path,
        "version": detection.version,
        "supports_no_alt_screen": detection.supports_no_alt_screen,
    }))
    .into_response()
}

pub(super) async fn prepare_agent_launch_args_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<PrepareAgentLaunchArgsRequest>,
) -> Response {
    if let Err(response) = require_local_or_auth(&addr, auth.is_some()) {
        return response.into_response();
    }
    Json(
        crate::agent_hook_launch::prepare_agent_launch_args(
            body.agent_type,
            body.binary_path,
            body.args,
        )
        .await,
    )
    .into_response()
}

pub(super) async fn detect_installed_ides_http() -> impl IntoResponse {
    Json(crate::agent::detect_installed_ides())
}

pub(super) async fn process_prompt_http(
    Json(body): Json<ProcessPromptRequest>,
) -> impl IntoResponse {
    Json(crate::prompt::process_prompt_content(
        body.content,
        body.variables,
    ))
}

pub(super) async fn extract_prompt_variables_http(
    Json(body): Json<ExtractVariablesRequest>,
) -> impl IntoResponse {
    Json(crate::prompt::extract_prompt_variables(body.content))
}

pub(super) async fn resolve_context_variables_http(
    Json(body): Json<serde_json::Value>,
) -> Response {
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    match crate::prompt::resolve_context_variables(repo_path).await {
        Ok(vars) => Json(vars).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn resolve_prompt_variables_http(Json(body): Json<serde_json::Value>) -> Response {
    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match crate::prompt::resolve_prompt_variables(content, repo_path).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn execute_headless_prompt_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    let command = match body
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        Some(c) => c.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "missing required field 'command'").into_response();
        }
    };
    let args: Vec<String> = body
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let stdin_content = body
        .get("stdinContent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let timeout_ms = body
        .get("timeoutMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(300_000);
    let repo_path = body
        .get("repoPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let env: Option<std::collections::HashMap<String, String>> = body
        .get("env")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    match crate::smart_prompt::execute_headless_prompt(
        command,
        args,
        stdin_content,
        timeout_ms,
        repo_path,
        env,
    )
    .await
    {
        Ok(output) => Json(output).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub(super) async fn verify_agent_session_http(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let agent_type = body
        .get("agentType")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let session_id = body
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cwd = body
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let agent_pid = body
        .get("agentPid")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let env_overrides = body
        .get("envOverrides")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Json(crate::agent_session::verify_agent_session(
        agent_type,
        session_id,
        cwd,
        agent_pid,
        env_overrides,
    ))
}

pub(super) async fn spawn_agent_session(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<SpawnAgentRequest>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if state.session_maps.sessions.len() >= MAX_CONCURRENT_SESSIONS {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "Max concurrent sessions reached"})),
        )
            .into_response();
    }

    // Determine binary path
    let binary_path = if let Some(ref path) = body.binary_path {
        let p = std::path::Path::new(path);
        if !p.is_absolute() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "binary_path must be an absolute path"})),
            )
                .into_response();
        }
        if !p.is_file() {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": "binary_path does not point to an existing file"}),
                ),
            )
                .into_response();
        }
        path.clone()
    } else if let Some(ref agent_type) = body.agent_type {
        let detection = crate::agent::detect_agent_binary(agent_type.clone()).await;
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": format!("Agent binary '{}' not found", agent_type)})),
                ).into_response()
            }
        }
    } else {
        // Default to claude
        let detection = crate::agent::detect_agent_binary("claude".to_string()).await;
        match detection.path {
            Some(p) => p,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code"})),
                ).into_response()
            }
        }
    };

    // Only Claude's CLI accepts a bare positional prompt. For other agents the
    // no-args default below would produce an argument-parse error and an immediate
    // exit (clap exit code 2), so reject up front with an actionable message rather
    // than spawning a doomed process.
    if body.args.is_none()
        && body
            .agent_type
            .as_deref()
            .is_some_and(|t| !crate::agent::agent_accepts_bare_prompt(t))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!(
                "Agent '{name}' cannot be spawned with a bare prompt (only Claude's CLI accepts a positional prompt; other agents exit with code 2). Pass explicit args, or spawn via the MCP agent tool with a configured run config.",
                name = body.agent_type.as_deref().unwrap_or_default()
            )})),
        )
            .into_response();
    }

    let rows = body.rows.unwrap_or(24);
    let cols = body.cols.unwrap_or(80);
    if let Err(msg) = super::validate_terminal_size(rows, cols) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response();
    }
    let session_id = Uuid::new_v4().to_string();
    if let Err(error) = crate::agent_capabilities::save_root(&state.data_dir, &session_id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":error})),
        )
            .into_response();
    }

    let attachment_dir =
        match crate::attachments::prepare_session_directory(&state.data_dir, "pty", &session_id) {
            Ok(directory) => directory,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error})),
                )
                    .into_response();
            }
        };

    let scoped_environment = match body
        .scope
        .as_deref()
        .map(|scope| state.secrets.scope_environment(scope))
        .transpose()
    {
        Ok(environment) => environment,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": error})),
            )
                .into_response();
        }
    };
    let scoped_environment = Arc::new(scoped_environment);
    let spawn_secrets = Arc::clone(&scoped_environment);
    let spawn_binary_path = binary_path.clone();
    let spawn_args = body.args.clone();
    let spawn_prompt = body.prompt.clone();
    let spawn_model = body.model.clone();
    let spawn_output_format = body.output_format.clone();
    let spawn_print_mode = body.print_mode;
    let spawn_cwd = body.cwd.clone();
    let spawn_env = body.env.clone();
    let spawn_state = Arc::clone(&state);
    let spawn_session_id = session_id.clone();
    let spawn_agent_type = body
        .agent_type
        .clone()
        .unwrap_or_else(|| "claude".to_string());
    let launch_receipt = Arc::new(Mutex::new(None));
    let captured_receipt = Arc::clone(&launch_receipt);
    let (pair, child) = match crate::pty::spawn_pty_pair_with_retry_async(
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        move || {
            let mut cmd = CommandBuilder::new(&spawn_binary_path);
            crate::pty::sanitize_pty_parent_env(&mut cmd);

            let mut launch_args = Vec::new();
            if let Some(ref args) = spawn_args {
                launch_args.extend(args.iter().cloned());
            } else {
                if spawn_print_mode.unwrap_or(false) {
                    launch_args.push("--print".to_string());
                }
                if let Some(ref format) = spawn_output_format {
                    launch_args.push("--output-format".to_string());
                    launch_args.push(format.clone());
                }
                if let Some(ref model) = spawn_model {
                    launch_args.push("--model".to_string());
                    launch_args.push(model.clone());
                }
                launch_args.push(spawn_prompt.clone());
            }
            crate::pty::apply_agent_screen_env(&mut cmd, &spawn_env);
            let final_args = crate::agent_hook_launch::launch_args(
                &spawn_agent_type,
                &spawn_binary_path,
                &launch_args,
                &crate::config::config_dir(),
                &attachment_dir,
            );
            crate::launch_instructions::apply_env(
                &mut cmd,
                &spawn_agent_type,
                &launch_args,
                &crate::config::config_dir(),
            );
            *captured_receipt.lock() = Some(crate::prompt_receipt::PromptReceipt::capture(
                &spawn_prompt,
                "TUIC HTTP agent spawn",
                &final_args,
                spawn_cwd.as_deref(),
                false,
            ));
            for arg in final_args {
                cmd.arg(arg);
            }

            for (key, value) in &spawn_env {
                cmd.env(key, value);
            }

            if let Some(environment) = spawn_secrets.as_ref() {
                environment.apply_to(&mut cmd);
            }

            crate::pty::bind_pty_identity(&spawn_state, &mut cmd, &spawn_session_id, None);

            if let Some(ref cwd) = spawn_cwd {
                cmd.cwd(crate::cli::expand_tilde(cwd));
            }
            cmd
        },
    )
    .await
    {
        Ok(pair_and_child) => pair_and_child,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY writer: {}", e)})),
            )
                .into_response();
        }
    };

    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            state.unbind_live_pty(&session_id);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to get PTY reader: {}", e)})),
            )
                .into_response();
        }
    };

    if let Some(environment) = scoped_environment.as_ref() {
        state.secrets.bind_scope(&session_id, environment);
    }
    let paused = Arc::new(AtomicBool::new(false));
    // Pre-set the session's agent type so the PTY reader's agent_active gate turns
    // on immediately and intent/suggest protocol tokens are parsed from the first
    // line of output. Seeded before registration because that is what publishes
    // `session-created`.
    let mut session_state = crate::state::SessionState {
        spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
        ..Default::default()
    };
    if let Some(ref agent_type) = body.agent_type {
        session_state.hook_instrumented = crate::pty::hook_instrumented_for(
            &crate::config::load_agents_config(),
            Some(agent_type.as_str()),
        );
        session_state.seed_configured_agent(Some(agent_type.clone()));
    }
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), session_state);

    // Buffers, alias, metrics, grid watch and the session-created broadcast,
    // sharing one helper with session::spawn_pty_session so the VT screen can only
    // ever be built at the geometry the PTY was opened with.
    super::session::register_pty_session(
        &state,
        &session_id,
        PtySession {
            launch_receipt: launch_receipt.lock().take(),
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            initial_cwd: body.cwd.clone(),
            cwd: body.cwd.clone(),
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: true,
            shell: binary_path.clone(),
        },
        rows,
        cols,
        body.agent_type.clone(),
        None,
        None,
    );

    crate::prompt_receipt::adopt_mcp_instructions(&state, &session_id);
    #[cfg(feature = "desktop")]
    let state_ref = state.clone();
    spawn_reader_thread(reader, paused, session_id.clone(), state, None);

    #[cfg(feature = "desktop")]
    if let Some(app) = state_ref.app_handle.read().as_ref() {
        super::session::emit_native_agent_session_created(
            |event, payload| {
                let _ = app.emit(event, payload);
            },
            &state_ref,
            &session_id,
            body.cwd.as_deref(),
            body.agent_type.as_deref(),
            None,
            None,
        );
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"session_id": session_id})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: HTTP/MCP readers start without the identity inherited by the
    // child, so a genuine native Codex hook cannot authorize its Chat transcript.
    #[cfg(unix)]
    #[tokio::test]
    async fn http_spawned_codex_pty_accepts_its_native_binding() {
        exercise_native_binding(false).await;
    }

    // Catches: session listing holds the PTY mutex while native binding
    // validation tries to acquire the same mutex, deadlocking HTTP and IPC.
    #[cfg(unix)]
    #[tokio::test]
    async fn bound_codex_session_listing_does_not_relock_pty() {
        exercise_native_binding(true).await;
    }

    #[cfg(unix)]
    async fn exercise_native_binding(check_listing: bool) {
        use base64::Engine;
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
        crate::config::save_agents_config(
            crate::config::AgentsConfig::default(),
            crate::config::AgentsConfig {
                launch_instructions: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        let cwd = scratch.path().canonicalize().unwrap();
        let mut hook: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/chat_view/codex/http-user-prompt-submit.json"
        ))
        .unwrap();
        hook["cwd"] = cwd.to_string_lossy().as_ref().into();
        hook["transcript_path"] = cwd.join("rollout.jsonl").to_string_lossy().as_ref().into();
        let mut header: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/chat_view/codex/session-meta.json"
        ))
        .unwrap();
        header["payload"]["id"] = hook["session_id"].clone();
        header["payload"]["cwd"] = hook["cwd"].clone();
        std::fs::write(
            hook["transcript_path"].as_str().unwrap(),
            format!(
                "{header}\n{}",
                include_str!("../fixtures/chat_view/codex/completed.jsonl")
            ),
        )
        .unwrap();
        // Like ForegroundIdentityProbe: this is an OS process-name probe,
        // not a fake implementation of Codex. The hook bytes are recorded.
        let probe = cwd.join("codex");
        std::fs::copy("/bin/cat", &probe).unwrap();
        #[cfg(target_os = "macos")]
        {
            let signed = std::process::Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&probe)
                .output()
                .unwrap();
            assert!(signed.status.success(), "sign the local process-name probe");
        }
        let frame = cwd.join("hook-frame");
        let encoded = base64::engine::general_purpose::STANDARD.encode(hook.to_string());
        let script = crate::test_support::fake_ssh_script(
            "codex-http-native-binding",
            "if [ \"$1\" = '--help' ]; then exit 0; fi; printf '\\033]7770;codex-session=%s:%s\\033\\\\\\nHOOK_REPLAY_DONE\\n' \"$TUIC_SESSION\" \"$CODEX_HOOK\" > \"$CODEX_FRAME\"; exec \"$CODEX_PROBE\" \"$CODEX_FRAME\" -",
            "exit /b 0",
        );
        let mut failures = Vec::new();
        for path in ["http", "mcp", "generic-restored"] {
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            let sid = match path {
                "http" => {
                    let body: SpawnAgentRequest = serde_json::from_value(serde_json::json!({
                        "prompt": "ignored", "agent_type": "codex", "args": [],
                        "binary_path": script, "cwd": cwd, "env": {"CODEX_HOOK": encoded, "CODEX_FRAME": frame, "CODEX_PROBE": probe}
                    })).unwrap();
                    let response = spawn_agent_session(
                        State(state.clone()),
                        ConnectInfo(loopback()),
                        None,
                        Json(body),
                    )
                    .await;
                    assert_eq!(response.status(), StatusCode::CREATED);
                    response_json(response).await["session_id"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                }
                "mcp" => {
                    let result = super::super::mcp_transport_session_agent::handle_agent(
                        &state,
                        loopback(),
                        &serde_json::json!({
                            "action": "spawn", "prompt": "ignored", "agent_type": "codex", "args": [],
                            "binary_path": script, "cwd": cwd, "env": {"CODEX_HOOK": encoded, "CODEX_FRAME": frame, "CODEX_PROBE": probe}
                        }),
                        None,
                    );
                    result["session_id"]
                        .as_str()
                        .unwrap_or_else(|| panic!("MCP spawn failed: {result}"))
                        .to_owned()
                }
                _ => super::super::session::spawn_pty_session(
                    state.clone(),
                    "/bin/sh".into(),
                    Some(cwd.to_string_lossy().into()),
                    24,
                    80,
                    None,
                    super::super::session::RequestedIdentity {
                        tuic_session: Some("restored-codex-owner".into()),
                        startup_command: Some(format!(
                            "CODEX_HOOK='{encoded}' CODEX_FRAME='{}' CODEX_PROBE='{}' exec '{}'",
                            frame.display(),
                            probe.display(),
                            script.display()
                        )),
                        agent_type: Some("codex".into()),
                        ..Default::default()
                    },
                )
                .expect("generic shell launch")
                .to_owned(),
            };
            let ready = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    if state.grid.vt_log_buffers.get(&sid).is_some_and(|vt| {
                        vt.lock()
                            .screen_rows()
                            .join("\n")
                            .contains("HOOK_REPLAY_DONE")
                    }) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await;
            // Wait for the reader to finish processing the chunk that painted the
            // marker, not for a protocol deadline: this is setup, not the subject.
            let result = if ready.is_ok() {
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        if state
                            .session_maps
                            .session_states
                            .get(&sid)
                            .is_some_and(|s| s.codex_binding.is_some())
                        {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
                .await
                .ok();
                // Observe the real exec'd process through the production probe;
                // the generic shell path must discover its agent, not hand-set it.
                super::super::session::get_foreground_process(
                    State(state.clone()),
                    axum::extract::Path(sid.clone()),
                )
                .await;
                crate::chat_view::chat_view_snapshot(&state, &sid, None, 0)
            } else {
                Err("hook replay never reached the reader".into())
            };
            if check_listing && result.is_ok() {
                let pid = state
                    .session_maps
                    .sessions
                    .get(&sid)
                    .unwrap()
                    .lock()
                    ._child
                    .process_id()
                    .unwrap();
                let listed_state = state.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let listing = std::thread::spawn(move || {
                    let rows = super::super::session::local_session_rows(&listed_state);
                    let mcp = super::super::mcp_transport_session_agent::handle_session(
                        &listed_state,
                        &serde_json::json!({"action":"list"}),
                        None,
                    );
                    let _ = tx.send((rows, mcp));
                });
                let (rows, mcp) = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                    Ok(rows) => rows,
                    Err(error) => {
                        // The failed worker owns the locked session. Do not
                        // deadlock cleanup on it: kill only our spawned probe.
                        unsafe {
                            libc::kill(pid as i32, libc::SIGKILL);
                        }
                        panic!("{path}: session listing deadlocked on native binding: {error}");
                    }
                };
                listing.join().unwrap();
                assert!(
                    mcp.as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row["session_id"] == sid),
                    "MCP must list the same bound PTY"
                );
                let row = rows.iter().find(|row| row.session_id == sid).unwrap();
                assert_eq!(
                    row.state
                        .as_ref()
                        .unwrap()
                        .codex_binding
                        .as_ref()
                        .unwrap()
                        .session_id,
                    hook["session_id"].as_str().unwrap()
                );
            }
            crate::pty::close_pty_core(&state, &sid, false);
            match result {
                Ok(snapshot) => assert!(
                    serde_json::to_string(&snapshot.updates)
                        .unwrap()
                        .contains("Recorded reply"),
                    "{path}: wrong transcript"
                ),
                Err(error) => failures.push(format!("{path}: {error}")),
            }
        }
        assert!(
            failures.is_empty(),
            "native binding lost: {}",
            failures.join("; ")
        );
    }

    fn spawn_request() -> SpawnAgentRequest {
        SpawnAgentRequest {
            scope: None,
            rows: Some(24),
            cols: Some(80),
            cwd: None,
            prompt: "test prompt".into(),
            model: None,
            print_mode: None,
            output_format: None,
            agent_type: None,
            binary_path: Some(
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            args: Some(vec!["--help".into()]),
            env: Default::default(),
        }
    }

    #[tokio::test]
    async fn http_typed_agent_launch_uses_the_same_rust_screen_policy() {
        let script = crate::test_support::fake_ssh_script(
            "http-terminal-screen-args",
            "printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let request = PrepareAgentLaunchArgsRequest {
            agent_type: "codex".into(),
            binary_path: script.to_string_lossy().into_owned(),
            args: vec!["resume".into()],
        };
        let response =
            prepare_agent_launch_args_http(ConnectInfo(loopback()), None, Json(request)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_json(response).await,
            serde_json::json!(["--no-alt-screen", "resume"])
        );

        let forbidden = PrepareAgentLaunchArgsRequest {
            agent_type: "codex".into(),
            binary_path: script.to_string_lossy().into_owned(),
            args: vec!["resume".into()],
        };
        let response =
            prepare_agent_launch_args_http(ConnectInfo(lan()), None, Json(forbidden)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn launch_args_request_rejects_removed_screen_override() {
        let request = serde_json::json!({
            "agentType": "codex",
            "binaryPath": "codex",
            "args": [],
            "allowAltScreen": true,
        });
        assert!(serde_json::from_value::<PrepareAgentLaunchArgsRequest>(request).is_err());
    }

    #[test]
    fn spawn_request_rejects_removed_screen_override() {
        for key in ["allow_alt_screen", "allowAltScreen"] {
            let mut request = serde_json::json!({
                "rows": 24,
                "cols": 80,
                "prompt": "work",
            });
            request[key] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<SpawnAgentRequest>(request).is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn spawn_request_accepts_browser_transport_body() {
        let body = include_str!("../../tests/fixtures/spawn_agent_http_body.json");
        let parsed: SpawnAgentRequest = serde_json::from_str(body).expect("HTTP spawn request");
        assert_eq!(parsed.rows, Some(30));
        assert_eq!(parsed.cols, Some(100));
        assert_eq!(parsed.cwd.as_deref(), Some("/agent"));
        assert_eq!(parsed.env.get("PROFILE").map(String::as_str), Some("work"));
    }

    #[tokio::test]
    async fn http_agent_spawn_uses_per_agent_screen_setting() {
        let script = crate::test_support::fake_ssh_script(
            "http-agent-screen-choice",
            "if [ \"$1\" = '--help' ]; then printf '%s\\n' '--no-alt-screen'; else printf 'ARGS=%s\\nTUIC_SESSION=%s\\n' \"$*\" \"$TUIC_SESSION\"; read unused; fi",
            "if \"%1\"==\"--help\" (echo --no-alt-screen) else (echo ARGS=%* & echo TUIC_SESSION=%TUIC_SESSION% & set /p HOLD=)",
        );
        // Launch instructions off: this test pins the argv around the screen flag.
        let config_root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(config_root.path().join("config"));
        crate::config::save_agents_config(
            crate::config::AgentsConfig::default(),
            crate::config::AgentsConfig {
                launch_instructions: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let body: SpawnAgentRequest = serde_json::from_value(serde_json::json!({
            "rows": 24,
            "cols": 80,
            "prompt": "ignored",
            "agent_type": "codex",
            "binary_path": script.to_string_lossy(),
            "args": ["resume"],
        }))
        .unwrap();
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let session_id = response_json(response).await["session_id"]
            .as_str()
            .unwrap()
            .to_string();
        // Catches: HTTP agent creation labels a direct program as a shell,
        // so its own foreground root would revoke the configured identity.
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(&session_id)
                .unwrap()
                .spawn_root_role,
            crate::state::SpawnRootRole::DirectProgram
        );
        assert!(
            state
                .session_maps
                .sessions
                .get(&session_id)
                .unwrap()
                .lock()
                ._child
                .process_id()
                .is_some()
        );
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(buffer) = state.grid.vt_log_buffers.get(&session_id) {
                    let text = buffer.lock().screen_rows().join("\n");
                    // PTY reads may split the two output lines. Wait for the identity
                    // line before checking either assertion; the first line is not readiness.
                    if text.contains(&format!("TUIC_SESSION={session_id}")) {
                        break text;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake agent output");
        assert!(output.contains("ARGS=--no-alt-screen resume"), "{output}");
        assert!(
            output.contains(&format!("TUIC_SESSION={session_id}")),
            "the spawned process must know its terminal identity: {output}"
        );
        let row = super::super::session::local_session_rows(&state)
            .into_iter()
            .find(|row| row.session_id == session_id)
            .unwrap();
        assert_eq!(row.tuic_session.as_deref(), Some(session_id.as_str()));
        super::super::session::close_session(State(state), axum::extract::Path(session_id)).await;
    }

    #[tokio::test]
    async fn http_agent_spawn_accepts_model_and_env_with_ipc_field_names() {
        let script = crate::test_support::fake_ssh_script(
            "http-agent-model-env",
            "printf 'ARG=%s\n' \"$@\"; printf 'SPAWN_VALUE=%s\nTUIC_SESSION=%s\n' \"$SPAWN_VALUE\" \"$TUIC_SESSION\"; read unused",
            "for %%a in (%*) do @echo ARG=%%~a
 echo SPAWN_VALUE=%SPAWN_VALUE% & echo TUIC_SESSION=%TUIC_SESSION% & set /p HOLD=",
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let body: SpawnAgentRequest = serde_json::from_value(serde_json::json!({
            "prompt": "task",
            "agent_type": "claude",
            "binary_path": script.to_string_lossy(),
            "model": "sonnet",
            "env": {"SPAWN_VALUE": "caller"},
        }))
        .unwrap();
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let session_id = response_json(response).await["session_id"]
            .as_str()
            .unwrap()
            .to_string();
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(buffer) = state.grid.vt_log_buffers.get(&session_id) {
                    let text = buffer.lock().screen_rows().join("\n");
                    if text.contains("SPAWN_VALUE=") {
                        break text;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake agent output");
        assert!(output.contains("ARG=--model"), "{output}");
        assert!(output.contains("ARG=sonnet"), "{output}");
        assert!(output.contains("ARG=task"), "{output}");
        assert!(output.contains("SPAWN_VALUE=caller"), "{output}");
        assert!(
            output.contains(&format!("TUIC_SESSION={session_id}")),
            "{output}"
        );
        super::super::session::close_session(State(state), axum::extract::Path(session_id)).await;
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn loopback() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
    fn lan() -> SocketAddr {
        "192.168.1.2:1".parse().unwrap()
    }

    fn authed() -> Option<Extension<Authenticated>> {
        Some(Extension(Authenticated))
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_rejects_unauthenticated_non_loopback() {
        let resp = execute_headless_prompt_http(
            ConnectInfo(lan()),
            None,
            Json(serde_json::json!({ "command": "echo", "args": ["x"] })),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_loopback_passes_guard() {
        // Loopback with missing 'command' field must yield 400 from the validator,
        // proving the 403 guard is NOT fired for loopback callers.
        let resp = execute_headless_prompt_http(
            ConnectInfo(loopback()),
            None,
            Json(serde_json::json!({})),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn execute_headless_prompt_http_authenticated_remote_passes_guard() {
        // Authenticated remote (full-trust, story 059) must pass the guard:
        // empty body yields 400 from the validator, not 403.
        let resp =
            execute_headless_prompt_http(ConnectInfo(lan()), authed(), Json(serde_json::json!({})))
                .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn spawn_agent_rejects_unauthenticated_remote_before_validation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = spawn_agent_session(
            State(state),
            ConnectInfo(lan()),
            None,
            Json(spawn_request()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn spawn_agent_rejects_relative_and_missing_binary_paths() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut relative = spawn_request();
        relative.binary_path = Some("bin/agent".into());
        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(relative),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await["error"],
            "binary_path must be an absolute path"
        );

        let mut missing = spawn_request();
        missing.binary_path = Some(
            std::env::temp_dir()
                .join("definitely-not-a-tuic-agent")
                .to_string_lossy()
                .into_owned(),
        );
        let response =
            spawn_agent_session(State(state), ConnectInfo(loopback()), None, Json(missing)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await["error"],
            "binary_path does not point to an existing file"
        );
    }

    #[tokio::test]
    async fn spawn_agent_rejects_non_claude_bare_prompt_before_pty_creation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut request = spawn_request();
        request.agent_type = Some("codex".into());
        request.args = None;

        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(request),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response_json(response).await["error"]
                .as_str()
                .unwrap()
                .contains("bare prompt")
        );
        assert!(state.session_maps.sessions.is_empty());
    }

    #[tokio::test]
    async fn spawn_agent_rejects_invalid_terminal_dimensions_before_pty_creation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut request = spawn_request();
        request.rows = Some(0);

        let response = spawn_agent_session(
            State(state.clone()),
            ConnectInfo(loopback()),
            None,
            Json(request),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response_json(response).await["error"].is_string());
        assert!(state.session_maps.sessions.is_empty());
    }
}

/// Body of `POST /agents/detect-all`.
#[derive(serde::Deserialize)]
pub(super) struct DetectAllBinariesRequest {
    pub binaries: Vec<String>,
}

/// `POST /agents/detect-all` — mirror of the `detect_all_agent_binaries`
/// command. Returns the same binary-name -> detection map.
pub(super) async fn detect_all_agent_binaries_http(
    Json(body): Json<DetectAllBinariesRequest>,
) -> impl IntoResponse {
    Json(crate::agent::detect_all_agent_binaries(body.binaries).await)
}

/// Body of `POST /agents/open-in-app`.
#[derive(serde::Deserialize)]
pub(super) struct OpenInAppRequest {
    pub path: String,
    pub app: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
}

/// `POST /agents/open-in-app` — mirror of the `open_in_app` command. The
/// command resolves to nothing, so this answers the `null` IPC returns.
pub(super) async fn open_in_app_http(Json(body): Json<OpenInAppRequest>) -> Response {
    super::json_result(crate::agent::open_in_app(
        body.path, body.app, body.line, body.col,
    ))
}
