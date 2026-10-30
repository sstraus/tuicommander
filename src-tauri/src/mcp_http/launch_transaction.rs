//! Durable at-most-once composition of workspace allocation and managed PTY spawn.
//! A checkpoint precedes each side effect. An uncertain checkpoint never retries
//! that side effect: a retained child is returned, otherwise the receipt blocks.
use crate::AppState;
use axum::{
    Json,
    extract::{ConnectInfo, State},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

#[path = "launch_transaction_acp.rs"]
mod acp_launch;
#[path = "launch_transaction_close.rs"]
mod owned_close;
pub(crate) use owned_close::{close_workflow_agent, workflow_agent_closes};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    idempotency_key: String,
    parent_session_id: String,
    name: String,
    keep_open: bool,
    workspace: Workspace,
    spawn: Value,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum Workspace {
    Existing(Existing),
    New(New),
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Existing {
    existing_path: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct New {
    repo_path: String,
    branch: String,
    base_ref: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Record {
    launch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_connection: Option<crate::acp::AcpConnectionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_conversation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_turn: Option<crate::acp::AcpTurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_prompt_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acp_prompt: Option<String>,
    digest: String,
    session_id: String,
    stage: String,
    workspace_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_baseline: Option<super::launch_baseline::WorkspaceBaseline>,
    child: Option<Value>,
    #[serde(default)]
    prompt_handoff: bool,
    reason: Option<String>,
    revision: i64,
}

fn hash(text: &[u8]) -> String {
    hex::encode(Sha256::digest(text))
}

// Sort object keys recursively: JSON member ordering is not a contract change.
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k, canonical(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

struct Journal(PathBuf);
impl Journal {
    fn open(&self) -> Result<Connection, String> {
        std::fs::create_dir_all(self.0.parent().unwrap()).map_err(|e| e.to_string())?;
        let db = Connection::open(&self.0).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS launches (key TEXT PRIMARY KEY, record TEXT NOT NULL, revision INTEGER NOT NULL);")
            .map_err(|e| e.to_string())?;
        Ok(db)
    }
    fn reserve(&self, key: &str, record: Record) -> Result<Record, String> {
        let db = self.open()?;
        let text = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        db.execute(
            "INSERT OR IGNORE INTO launches VALUES (?1, ?2, 0)",
            params![key, text],
        )
        .map_err(|e| e.to_string())?;
        let text: String = db
            .query_row("SELECT record FROM launches WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
    fn get(&self, id: &str) -> Result<Option<Record>, String> {
        let text: Option<String> = self
            .open()?
            .query_row(
                "SELECT record FROM launches WHERE json_extract(record, '$.launch_id')=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        text.map(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .transpose()
    }
    fn key_for_id(&self, id: &str) -> Result<String, String> {
        self.open()?
            .query_row(
                "SELECT key FROM launches WHERE json_extract(record, '$.launch_id')=?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| format!("find managed launch key: {e}"))
    }
    fn save(&self, key: &str, record: &mut Record) -> Result<(), String> {
        let revision = record.revision;
        record.revision += 1;
        let text = serde_json::to_string(record).map_err(|e| e.to_string())?;
        let changed = self
            .open()?
            .execute(
                "UPDATE launches SET record=?1, revision=?2 WHERE key=?3 AND revision=?4",
                params![text, record.revision, key, revision],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("launch changed concurrently; repeat the same request".into());
        }
        Ok(())
    }
}

pub(crate) async fn execute(state: &Arc<AppState>, addr: SocketAddr, args: &Value) -> Value {
    execute_inner(state, addr, args, Fault::None)
        .await
        .unwrap_or_else(|error| json!({"error": error}))
}

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    #[cfg(test)]
    AfterWorktree,
    #[cfg(test)]
    AfterWorktreeEffect,
    #[cfg(test)]
    AfterChild,
    #[cfg(test)]
    AfterBaseline,
    #[cfg(test)]
    DuringDelivery,
}

async fn execute_inner(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &Value,
    fault: Fault,
) -> Result<Value, String> {
    execute_scoped(state, addr, args, fault, true).await
}

/// Database-owned automation/workflow runs have an owner scope, not a fake peer.
/// Only in-process callers can use this entry; transport requests require a peer.
pub(super) async fn execute_owned(state: &Arc<AppState>, addr: SocketAddr, args: &Value) -> Value {
    execute_scoped(state, addr, args, Fault::None, false)
        .await
        .unwrap_or_else(|error| json!({"error":error}))
}

async fn execute_scoped(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &Value,
    fault: Fault,
    require_peer: bool,
) -> Result<Value, String> {
    if !addr.ip().is_loopback() {
        return Err("Managed launch is restricted to localhost connections".into());
    }
    if let Some(id) = args.get("launch_id") {
        let id = id.as_str().ok_or("launch_id must be a string")?;
        if args.as_object().is_none_or(|map| {
            map.keys()
                .any(|key| !matches!(key.as_str(), "launch_id" | "action"))
        }) {
            return Err("launch_id reads cannot include launch mutations".into());
        }
        let _guard = state.launch_transaction_lock.lock().await;
        let record = Journal(state.data_dir.join("managed-launches.sqlite"))
            .get(id)?
            .ok_or("Unknown launch_id")?;
        return Ok(receipt(state, &record));
    }
    let mut body = args.clone();
    if let Some(map) = body.as_object_mut() {
        map.remove("action");
    }
    let request: Request =
        serde_json::from_value(body).map_err(|e| format!("Invalid launch request: {e}"))?;
    if request.idempotency_key.trim().is_empty() || request.idempotency_key.len() > 256 {
        return Err("idempotency_key must contain 1..256 bytes".into());
    }
    if request.name.trim().is_empty() || request.name != request.name.trim() {
        return Err("name must be non-empty with no surrounding whitespace".into());
    }
    if !request.spawn.is_object() {
        return Err("spawn must be an object of agent spawn arguments".into());
    }
    for reserved in [
        "action",
        "name",
        "keep_open",
        "cwd",
        "parent_session_id",
        "idempotency_key",
    ] {
        if request.spawn.get(reserved).is_some() {
            return Err(format!(
                "spawn.{reserved} is owned by the launch transaction"
            ));
        }
    }
    if request.spawn["prompt"].as_str().is_none() {
        return Err("spawn.prompt is required".into());
    }
    let acp = request
        .spawn
        .get("acp")
        .map(|value| {
            serde_json::from_value::<acp_launch::AcpLaunch>(value.clone())
                .map_err(|e| format!("invalid ACP launch: {e}"))
        })
        .transpose()?;
    if acp.is_some() && (require_peer || !request.parent_session_id.starts_with("workflow:")) {
        return Err("ACP managed launch requires daemon workflow authority".into());
    }
    let digest =
        hash(&serde_json::to_vec(&canonical(serde_json::to_value(&request).unwrap())).unwrap());
    let key = hash(
        &serde_json::to_vec(&(
            if require_peer { "peer" } else { "host" },
            &request.parent_session_id,
            &request.idempotency_key,
        ))
        .unwrap(),
    );
    let _guard = state.launch_transaction_lock.lock().await;
    let journal = Journal(state.data_dir.join("managed-launches.sqlite"));
    let mut record = journal.reserve(
        &key,
        Record {
            launch_id: uuid::Uuid::new_v4().to_string(),
            owner_scope: Some(request.parent_session_id.clone()),
            acp_prompt: acp
                .as_ref()
                .map(|_| request.spawn["prompt"].as_str().unwrap().to_owned()),
            acp_connection: None,
            acp_process_id: None,
            acp_generation: None,
            acp_conversation: None,
            acp_turn: None,
            acp_prompt_sha256: acp
                .as_ref()
                .map(|_| hash(request.spawn["prompt"].as_str().unwrap().as_bytes())),
            digest: digest.clone(),
            session_id: uuid::Uuid::new_v4().to_string(),
            stage: "reserved".into(),
            workspace_path: None,
            workspace_baseline: None,
            child: None,
            prompt_handoff: false,
            reason: None,
            revision: 0,
        },
    )?;
    if record.digest != digest {
        return Err("idempotency_conflict: key already names a different launch request".into());
    }
    if matches!(
        record.stage.as_str(),
        "created" | "blocked" | "acp_not_delivered" | "acp_prompt_started" | "acp_delivered"
    ) {
        return Ok(receipt(state, &record));
    }
    if record.stage == "spawn_started" {
        // Creation may have completed before its reply/checkpoint was lost. Never
        // spawn again, even when the child has since exited or TUIC restarted.
        if state.session_maps.sessions.contains_key(&record.session_id) {
            record.stage = "created".into();
            record.prompt_handoff = has_sent_brief(state, &record.session_id);
        } else {
            record.stage = "blocked".into();
            record.reason = Some("child_creation_uncertain: inspect the retained identity; no replacement was created".into());
        }
        journal.save(&key, &mut record)?;
        return Ok(receipt(state, &record));
    }
    if record.stage == "worktree_started" {
        record.stage = "blocked".into();
        record.reason = Some("worktree_creation_uncertain: inspect repository worktrees; no fallback checkout was used".into());
        journal.save(&key, &mut record)?;
        return Ok(receipt(state, &record));
    }
    if require_peer && !state.peer_agents.contains_key(&request.parent_session_id) {
        record.stage = "blocked".into();
        record.reason = Some("parent_session_id must identify a registered peer".into());
        journal.save(&key, &mut record)?;
        return Ok(receipt(state, &record));
    }
    if record.stage == "reserved" {
        match &request.workspace {
            Workspace::Existing(existing) => {
                let path = std::fs::canonicalize(crate::cli::expand_tilde(&existing.existing_path))
                    .map_err(|e| format!("existing workspace: {e}"))?;
                // Verify the selected directory is a Git checkout, not a guessed
                // fallback cwd. No process changes cwd and no branch is switched.
                if tuic_git::git::resolve_git_dir(&path).is_none() {
                    return Err("existing_path must name a Git checkout root".into());
                }
                record.workspace_path = Some(path.to_string_lossy().into_owned());
            }
            Workspace::New(new) => {
                if new.branch.trim().is_empty() {
                    return Err("workspace.branch must be non-empty".into());
                }
                record.stage = "worktree_started".into();
                journal.save(&key, &mut record)?;
                match super::worktree_routes::create_worktree_shared(
                    state,
                    new.repo_path.clone(),
                    new.branch.clone(),
                    new.base_ref.clone(),
                    Some(request.parent_session_id.clone()),
                    true,
                )
                .await
                {
                    Ok(created) => {
                        record.workspace_path = Some(created.path);
                        #[cfg(test)]
                        if fault == Fault::AfterWorktreeEffect {
                            return Err("injected interruption before worktree checkpoint".into());
                        }
                    }
                    Err((_, body)) => {
                        record.stage = "blocked".into();
                        record.reason = Some(
                            body.0["error"]
                                .as_str()
                                .unwrap_or("worktree creation refused")
                                .to_string(),
                        );
                        journal.save(&key, &mut record)?;
                        return Ok(receipt(state, &record));
                    }
                }
            }
        }
        record.stage = "selected".into();
        journal.save(&key, &mut record)?;
    }
    #[cfg(test)]
    if fault == Fault::AfterWorktree {
        return Err("injected lost reply after worktree selection".into());
    }
    if !require_peer
        && request.parent_session_id.starts_with("workflow:")
        && record.workspace_baseline.is_none()
    {
        let path = PathBuf::from(
            record
                .workspace_path
                .as_deref()
                .ok_or("missing workflow workspace")?,
        );
        record.workspace_baseline = Some(
            tokio::task::spawn_blocking(move || super::launch_baseline::capture(&path))
                .await
                .map_err(|error| format!("capture workspace baseline: {error}"))??,
        );
        // The original baseline commits before spawn. A retry after selection
        // must never replace pre-existing ownership with the worker's edits.
        journal.save(&key, &mut record)?;
    }
    #[cfg(test)]
    if fault == Fault::AfterBaseline {
        return Err("injected interruption after workspace baseline".into());
    }
    if let Some(acp) = acp {
        match acp_launch::create(state, &journal, &key, &mut record, &acp).await {
            Ok(()) => record.reason = None,
            Err(error) => record.reason = Some(error),
        }
        journal.save(&key, &mut record)?;
        #[cfg(test)]
        if fault == Fault::AfterChild {
            return Err("injected lost ACP spawn response".into());
        }
        return Ok(receipt(state, &record));
    }
    record.stage = "spawn_started".into();
    journal.save(&key, &mut record)?;
    let mut spawn = request.spawn.clone();
    spawn["action"] = json!("spawn");
    spawn["name"] = json!(request.name);
    spawn["keep_open"] = json!(request.keep_open);
    spawn["cwd"] = json!(record.workspace_path);
    let child_state = Arc::clone(state);
    let session = record.session_id.clone();
    let parent = request.parent_session_id.clone();
    let child = tokio::task::spawn_blocking(move || {
        super::mcp_transport_session_agent::spawn_with_identity(
            &child_state,
            addr,
            &spawn,
            None,
            None,
            Some((&session, require_peer.then_some(parent.as_str()))),
        )
    })
    .await
    .map_err(|e| format!("child creation uncertain: {e}"))?;
    #[cfg(test)]
    if fault == Fault::AfterChild {
        return Err("injected lost reply after child creation".into());
    }
    if let Some(error) = child["error"].as_str() {
        record.stage = "blocked".into();
        record.reason = Some(error.to_string());
    } else {
        record.stage = "created".into();
        record.child = Some(child);
        record.prompt_handoff = has_sent_brief(state, &record.session_id);
    }
    journal.save(&key, &mut record)?;
    #[cfg(test)]
    if fault == Fault::DuringDelivery {
        crate::pty::notify_initial_prompt_timeout_if_pending(state, &record.session_id);
    }
    let _ = fault;
    Ok(receipt(state, &record))
}

/// Only the workflow binding path may release the initial ACP prompt.
pub(super) async fn deliver_acp_owned(state: &Arc<AppState>, id: &str) -> Result<Value, String> {
    let _guard = state.launch_transaction_lock.lock().await;
    let journal = Journal(state.data_dir.join("managed-launches.sqlite"));
    let key = journal.key_for_id(id)?;
    let mut record = journal.get(id)?.ok_or("unknown managed launch")?;
    let prompt = record
        .acp_prompt
        .clone()
        .ok_or("ACP launch has no retained pending prompt")?;
    if record.acp_prompt_sha256.as_deref() != Some(hash(prompt.as_bytes()).as_str()) {
        return Err("ACP delivery payload differs from the pinned launch prompt".into());
    }
    match acp_launch::deliver(state, &journal, &key, &mut record, &prompt).await {
        Ok(()) => record.reason = None,
        Err(error) => record.reason = Some(error),
    }
    journal.save(&key, &mut record)?;
    Ok(receipt(state, &record))
}

fn acp_receipt(state: &AppState, record: &Record) -> Value {
    let observed = state.acp.peer_snapshot(&record.session_id).filter(|s| {
        Some(s.connection_id) == record.acp_connection
            && Some(s.generation) == record.acp_generation
    });
    let live = observed.as_ref().is_some_and(|s| s.settlement.is_none());
    let delivery = match record.stage.as_str() {
        "acp_not_delivered" => "not_delivered",
        "acp_delivered" => "delivered",
        _ => "unknown",
    };
    let reason = record.reason.as_deref().or_else(|| {
        (!live).then_some(
            "matching ACP inventory is unavailable or settled; inspect the durable references",
        )
    });
    json!({
        "launch_id":record.launch_id, "session_id":null, "peer_id":record.session_id,
        "transport":"acp", "stage":record.stage, "status":if reason.is_some() {"blocked"} else {"created"},
        "live":live, "inventory":if observed.is_some() {"present"} else {"unknown"},
        "prompt_delivery":delivery, "ready":false, "reason":reason,
        "connection_id":record.acp_connection, "generation":record.acp_generation,
        "conversation_id":record.acp_conversation, "turn_id":record.acp_turn,
        "settlement":observed.and_then(|s| s.settlement),
        "workspace_path":record.workspace_path, "workspace_baseline":record.workspace_baseline,
        "child":record.child,
    })
}

fn has_sent_brief(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .sessions
        .get(session_id)
        .is_some_and(|session| {
            session
                .lock()
                .launch_receipt
                .as_ref()
                .is_some_and(|receipt| {
                    receipt
                        .sections
                        .iter()
                        .any(|section| section.label == "Launch brief" && section.status == "sent")
                })
        })
}

fn receipt(state: &AppState, record: &Record) -> Value {
    if record.acp_prompt_sha256.is_some() {
        return acp_receipt(state, record);
    }
    let live = state.session_maps.sessions.contains_key(&record.session_id);
    let pending = state
        .pending_initial_prompts
        .get(&record.session_id)
        .map(|p| p.value().clone());
    let session = state.session_state_with_shell(&record.session_id);
    let delivery = if pending.is_some() {
        "queued"
    } else if record.prompt_handoff || has_sent_brief(state, &record.session_id) {
        "delivered"
    } else {
        "unknown"
    };
    let blocked = record.stage == "blocked"
        || !live
        || pending.as_ref().is_some_and(|p| p.notified)
        || session.as_ref().is_some_and(|s| s.awaiting_input);
    let ready = crate::pty::managed_launch_screen_ready(state, &record.session_id)
        && live
        && delivery == "delivered"
        && !blocked
        && session
            .as_ref()
            .is_some_and(|s| matches!(s.agent_state.as_deref(), Some("idle" | "completed")));
    let status = if blocked {
        "blocked"
    } else if ready {
        "ready"
    } else if delivery == "delivered" {
        "prompt_delivered"
    } else {
        "created"
    };
    json!({
        "launch_id": record.launch_id, "session_id": record.session_id,
        "status": status, "stage": record.stage, "live": live,
        "prompt_delivery": delivery, "ready": ready,
        "reason": record.reason.as_deref().or_else(|| if !live && record.stage == "created" { Some("child is no longer live; this key never creates a replacement") } else if pending.as_ref().is_some_and(|p| p.notified) { Some("initial prompt delivery timed out; prompt remains queued") } else if blocked { Some(if live { "child awaits input" } else { "no live child; repeat the original request only to resume a safe checkpoint" }) } else { None }),
        "workspace_path": record.workspace_path,
        "workspace_baseline": record.workspace_baseline,
        "warm_artifacts": record.workspace_path.as_ref().map(|path| crate::worktree::warm_status(std::path::Path::new(path))),
        "child": record.child,
    })
}

pub(super) async fn http(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<Value>,
) -> Json<Value> {
    Json(execute(&state, addr, &request).await)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn managed_agent_launch(
    state: tauri::State<'_, Arc<AppState>>,
    request: Value,
) -> Result<Value, String> {
    let state = Arc::clone(state.inner());
    Ok(execute(&state, SocketAddr::from(([127, 0, 0, 1], 0)), &request).await)
}

#[cfg(test)]
#[path = "launch_transaction_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "launch_transaction_acp_tests.rs"]
mod acp_tests;
