//! Workflow process closure is independent of artifact approval and workspace
//! removal. A refused/uncertain stop leaves both evidence and workspace intact.
use super::*;
use crate::workflows::{AttemptState, RunSnapshot, RunStatus};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseRecord {
    request_sha256: String,
    run_id: String,
    attempt_id: String,
    successor_attempt_id: Option<String>,
    launch_id: String,
    state: String,
    reason: Option<String>,
    process_id: Option<u32>,
    workspace_before: Option<super::super::launch_baseline::WorkspaceBaseline>,
    stop_receipt: Option<Value>,
}

fn read(db: &Connection, launch: &str, request: &str) -> Result<Option<CloseRecord>, String> {
    let text: Option<String> = db
        .query_row(
            "SELECT document FROM workflow_closes WHERE launch_id=?1 AND request_id=?2",
            params![launch, request],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    text.map(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
        .transpose()
}

fn write(db: &Connection, request: &str, close: &CloseRecord) -> Result<Value, String> {
    let text = serde_json::to_string(close).map_err(|e| e.to_string())?;
    db.execute("INSERT INTO workflow_closes VALUES (?1,?2,?3) ON CONFLICT(launch_id,request_id) DO UPDATE SET document=excluded.document",
        params![close.launch_id, request, text]).map_err(|e| e.to_string())?;
    serde_json::to_value(close).map_err(|e| e.to_string())
}

fn successor(run: &RunSnapshot, attempt_id: &str, next: Option<&str>) -> Result<(), String> {
    let attempt = run
        .attempts
        .iter()
        .find(|a| a.id == attempt_id)
        .ok_or("attempt missing")?;
    if attempt.state == AttemptState::Running || attempt.report.is_none() {
        return Err(
            "an idle screen is not a terminal report; the attempt still owns its process".into(),
        );
    }
    if let Some(next) = next {
        let next = run
            .attempts
            .iter()
            .find(|a| a.id == next && a.id != attempt_id)
            .ok_or("successor attempt missing")?;
        if next.story_id != attempt.story_id
            || next.agent.is_none()
            || next.state != AttemptState::Running
            || run.attempts.iter().position(|a| a.id == next.id)
                <= run.attempts.iter().position(|a| a.id == attempt_id)
        {
            return Err("successor is not durably bound to the same target".into());
        }
    } else if !matches!(run.status, RunStatus::Completed | RunStatus::Cancelled) {
        return Err("a nonterminal run requires a durably bound successor".into());
    }
    Ok(())
}

fn background_refusal(commands: Option<&[String]>, peer: &str) -> Result<(), String> {
    let commands = commands.ok_or("process inventory unknown; no stop authorized")?;
    // On Windows the process snapshot deliberately has no argv. It cannot prove
    // that a detached runner/test app is absent.
    if commands.iter().any(String::is_empty) {
        return Err("process argv inventory unknown; no stop authorized".into());
    }
    if crate::idle_close::live_bg_runner_for_session(commands, peer) {
        return Err("live background receipt prevents owned close".into());
    }
    // Test-instance executables encode their instance in the filename. A test
    // app may be detached from the worker tree, so inspect the complete inventory.
    if commands.iter().any(|line| {
        line.split_whitespace().next().is_some_and(|exe| {
            let name = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
            name.starts_with("tuicommander-agent-test") || name.starts_with("tuic-agent-test")
        })
    }) {
        return Err("live isolated test app prevents owned close".into());
    }
    Ok(())
}

fn pty_refusal(state: &AppState, peer: &str) -> Result<(), String> {
    let snapshot = state
        .session_state_with_shell(peer)
        .ok_or("owned process inventory unknown")?;
    if !matches!(snapshot.agent_state.as_deref(), Some("idle" | "completed"))
        || snapshot.awaiting_input
        || snapshot.background_work
        || snapshot.has_pending_background_probe()
        || crate::idle_close::bg_wake_blocks_close(peer)
        || state.pending_initial_prompts.contains_key(peer)
        || crate::pty::queued_command_count(state, peer) > 0
        || state
            .session_maps
            .input_buffers
            .get(peer)
            .is_some_and(|b| !b.lock().is_empty())
    {
        return Err("owned process has live work, a draft, a question or undelivered input".into());
    }
    let cursor = state.agent_read_cursor.get(peer).map(|v| *v).unwrap_or(0);
    if state
        .agent_inbox
        .get(peer)
        .is_some_and(|mail| mail.iter().any(|m| m.timestamp > cursor))
    {
        return Err("owned process has unread mail".into());
    }
    Ok(())
}

fn acp_refusal(
    state: &AppState,
    record: &Record,
    binding: &crate::workflows::AcpAgentBinding,
    processes: &[crate::pty::ProcessTreeEntry],
) -> Result<(), String> {
    let snapshot = state
        .acp
        .snapshot(binding.connection_id)
        .map_err(|e| e.message)?;
    let [attachment] = snapshot.attachments.as_slice() else {
        return Err("ACP attachment inventory unknown".into());
    };
    if snapshot.generation != binding.generation
        || snapshot.settlement.is_some()
        || attachment.session_id.to_string() != binding.conversation_id
        || !attachment.is_quiescent()
    {
        return Err("ACP conversation has live work or unknown inventory".into());
    }
    let pid = record
        .acp_process_id
        .ok_or("ACP process-stop identity was not recorded")?;
    if crate::pty::workflow_acp_process(&record.session_id, processes)? != pid
        || !crate::pty::workflow_acp_tree_quiet(processes, pid)
    {
        return Err("ACP owned root changed or has live descendants".into());
    }
    let cursor = state
        .agent_read_cursor
        .get(&record.session_id)
        .map(|v| *v)
        .unwrap_or(0);
    if state
        .agent_inbox
        .get(&record.session_id)
        .is_some_and(|mail| mail.iter().any(|m| m.timestamp > cursor))
    {
        return Err("ACP worker has unread mail".into());
    }
    Ok(())
}

pub(crate) fn workflow_agent_closes(state: &AppState, run: &RunSnapshot) -> Result<Value, String> {
    let path = state.data_dir.join("managed-launches.sqlite");
    if !path.exists() {
        return Ok(json!([]));
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_closes')", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !exists {
        return Ok(json!([]));
    }
    let mut statement = db.prepare("SELECT document FROM workflow_closes WHERE json_extract(document,'$.runId')=?1 ORDER BY rowid").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([&run.id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let closes = rows
        .map(|row| {
            serde_json::from_str::<Value>(&row.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!(closes))
}

/// The shared run adapter calls this on a blocking worker. Repeating a close
/// reconciles its recorded stop, never kills a replacement or removes a checkout.
pub(crate) fn close_workflow_agent(
    state: &AppState,
    run: &RunSnapshot,
    attempt_id: &str,
    request_id: &str,
    expected_sequence: i64,
    next: Option<&str>,
) -> Result<Value, String> {
    if request_id.trim().is_empty() || request_id.len() > 256 {
        return Err("close request_id must contain 1..256 bytes".into());
    }
    state.workflow_runtime.require_owner()?;
    let _guard = state
        .launch_transaction_lock
        .try_lock()
        .map_err(|_| "managed launch/close is active; retry the same request")?;
    let attempt = run
        .attempts
        .iter()
        .find(|a| a.id == attempt_id)
        .ok_or("attempt missing")?;
    let binding = attempt.agent.as_ref().ok_or("attempt has no owned agent")?;
    let id = binding
        .launch_id
        .as_deref()
        .ok_or("historical binding has no durable launch authority")?;
    let journal = Journal(state.data_dir.join("managed-launches.sqlite"));
    let record = journal
        .get(id)?
        .ok_or("managed launch missing; ownership unknown")?;
    if record.owner_scope.as_deref() != Some(format!("workflow:{}", run.id).as_str())
        || record.session_id != binding.session_id
    {
        return Err("managed launch is not owned by this run/attempt".into());
    }
    let db = journal.open()?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS workflow_closes (launch_id TEXT NOT NULL, request_id TEXT NOT NULL, document TEXT NOT NULL, PRIMARY KEY(launch_id, request_id));")
        .map_err(|e| e.to_string())?;
    let digest = hash(
        &serde_json::to_vec(&(run.id.as_str(), attempt_id, next, expected_sequence))
            .map_err(|e| e.to_string())?,
    );
    if let Some(mut close) = read(&db, id, request_id)? {
        if close.request_sha256 != digest {
            return Err("idempotency_conflict: close request changed".into());
        }
        if close.state == "stop_requested" {
            // Loss of the PTY map alone is not a stop receipt. Require a fresh,
            // complete OS inventory proving the exact recorded process absent.
            if let (Some(pid), Some(processes)) =
                (close.process_id, crate::pty::process_tree_snapshot())
                && !crate::pty::workflow_process_observed(&processes, pid)
            {
                close.state = "closed".into();
                close.stop_receipt = Some(json!({"processId":pid,"observedAbsent":true}));
                close.reason = None;
                return write(&db, request_id, &close);
            }
        }
        return serde_json::to_value(close).map_err(|e| e.to_string());
    }
    if run.sequence != expected_sequence {
        return Err("close sequence changed; refresh the run".into());
    }
    let mut close = CloseRecord {
        request_sha256: digest,
        run_id: run.id.clone(),
        attempt_id: attempt_id.into(),
        successor_attempt_id: next.map(str::to_owned),
        launch_id: id.into(),
        state: "refused".into(),
        reason: None,
        process_id: None,
        workspace_before: None,
        stop_receipt: None,
    };
    let ready = (|| {
        successor(run, attempt_id, next)?;
        let baseline = record
            .workspace_baseline
            .as_ref()
            .ok_or("launch baseline missing")?;
        let path = record
            .workspace_path
            .as_deref()
            .ok_or("owned workspace missing")?;
        let current = super::super::launch_baseline::capture(std::path::Path::new(path))?;
        close.workspace_before = Some(current.clone());
        if current.repository_key != baseline.repository_key
            || current.workspace_key != baseline.workspace_key
        {
            return Err("workspace identity changed; no close authorized".to_owned());
        }
        if !current.status_hex.is_empty() || !baseline.status_hex.is_empty() {
            return Err(
                "unattributed or pre-existing dirty work is retained; no close authorized".into(),
            );
        }
        if let Some(next) = next {
            let next_binding = run
                .attempts
                .iter()
                .find(|a| a.id == next)
                .and_then(|a| a.agent.as_ref())
                .ok_or("successor binding missing")?;
            let next_record = journal
                .get(
                    next_binding
                        .launch_id
                        .as_deref()
                        .ok_or("successor launch missing")?,
                )?
                .ok_or("successor launch inventory unknown")?;
            let next_receipt = receipt(state, &next_record);
            if next_receipt["live"] != true || next_receipt["prompt_delivery"] != "delivered" {
                return Err(
                    "successor delivery or live process inventory is not established".into(),
                );
            }
            if next_record.owner_scope != record.owner_scope
                || next_record.session_id != next_binding.session_id
                || next_record.workspace_baseline.as_ref().is_none_or(|b| {
                    b.workspace_key != baseline.workspace_key
                        || b.repository_key != baseline.repository_key
                })
            {
                return Err("successor has no matching workspace ownership receipt".into());
            }
        }
        let processes = crate::pty::process_tree_snapshot().ok_or("process inventory unknown")?;
        let commands = processes
            .iter()
            .map(|p| p.command.clone())
            .collect::<Vec<_>>();
        background_refusal(Some(&commands), &binding.session_id)?;
        if let Some(acp) = &binding.acp {
            acp_refusal(state, &record, acp, &processes)?;
            close.process_id = record.acp_process_id;
        } else {
            pty_refusal(state, &binding.session_id)?;
            if !crate::pty::workflow_close_tree_quiet(state, &binding.session_id, &processes) {
                return Err("owned process tree has live or unknown descendants".into());
            }
            close.process_id = state
                .session_maps
                .sessions
                .get(&binding.session_id)
                .and_then(|s| s.lock()._child.process_id());
        }
        if close.process_id.is_none() {
            return Err("owned process identity unknown".into());
        }
        // Re-read the authoritative ledger and draft/state after slow Git/OS probes.
        if crate::workflows::RunStore::open()?
            .snapshot(&run.id)?
            .sequence
            != expected_sequence
        {
            return Err("run changed during close preflight".into());
        }
        if let Some(acp) = &binding.acp {
            let processes =
                crate::pty::process_tree_snapshot().ok_or("process inventory unknown")?;
            acp_refusal(state, &record, acp, &processes)?;
        } else {
            pty_refusal(state, &binding.session_id)?;
        }
        Ok(())
    })();
    if let Err(reason) = ready {
        close.reason = Some(reason);
        return write(&db, request_id, &close);
    }
    close.state = "stop_requested".into();
    write(&db, request_id, &close)?;
    // Never invoke worktree cleanup, even for a clean checkout. WF07 owns
    // integration/archive/removal authority, independently of this process stop.
    if let Some(acp) = &binding.acp {
        let runtime = tokio::runtime::Handle::try_current().map_err(|e| e.to_string())?;
        let settlement = runtime.block_on(state.acp.disconnect(acp.connection_id));
        if let Err(error) = settlement {
            close.reason = Some(error.message);
        }
    } else {
        crate::pty::close_pty_core_with_reason(
            state,
            &binding.session_id,
            false,
            "workflow_owned_close",
        );
    }
    if let (Some(pid), Some(processes)) = (close.process_id, crate::pty::process_tree_snapshot())
        && !crate::pty::workflow_process_observed(&processes, pid)
    {
        close.state = "closed".into();
        close.stop_receipt = Some(json!({"processId":pid,"observedAbsent":true}));
    } else {
        close.reason = Some("stop requested; process absence is not yet observed".into());
    }
    write(&db, request_id, &close)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: idle reap treating a detached receipt as no live work, or a
    // failed/partial inventory as an empty process list.
    #[test]
    fn live_background_receipt_prevents_idle_reap() {
        assert!(background_refusal(None, "worker").is_err());
        assert!(background_refusal(Some(&[String::new()]), "worker").is_err());
        assert!(
            background_refusal(
                Some(&["tuic __bg-runner worker -- cargo test".into()]),
                "worker"
            )
            .unwrap_err()
            .contains("background receipt")
        );
        assert!(
            background_refusal(
                Some(&["/tmp/tuicommander-agent-test-wf04".into()]),
                "worker"
            )
            .unwrap_err()
            .contains("test app")
        );
        assert!(
            background_refusal(
                Some(&["tuic __bg-runner other -- cargo test".into()]),
                "worker"
            )
            .is_ok()
        );
    }
}
