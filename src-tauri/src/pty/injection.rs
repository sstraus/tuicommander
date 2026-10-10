use super::*;

pub(super) struct ParentLifecycleDispatch {
    pub(super) parent_id: String,
    pub(super) message_id: String,
    message_timestamp: u64,
    pub(super) framed: String,
}

/// Render one lifecycle payload as a single human-facing line, without the
/// `[TUIC] ` marker so it also composes into a multi-event summary.
///
/// Shared by the direct framed delivery and the orchestrator summary notice:
/// the two describe the same events from different sources (the payload being
/// enqueued vs. the copy read back out of the inbox) and must never word them
/// differently.
///
/// The result is injected into an agent's composer, so it MUST stay one short
/// line — a multi-line paste submits itself halfway through.
pub(super) fn describe_lifecycle_payload(
    child_session: &str,
    payload: &serde_json::Value,
) -> String {
    let child = short_session(child_session);
    if payload.get("type").and_then(|t| t.as_str()) == Some("prompt_delivered") {
        return format!("child agent {child} has taken its initial prompt after all");
    }
    if payload.get("type").and_then(|t| t.as_str()) == Some("prompt_delivery_failed") {
        return match payload.get("reason").and_then(|r| r.as_str()) {
            Some("startup_dialog") => format!(
                "child agent {child} is stalled on a startup dialog; its prompt is queued and will be typed once it is answered"
            ),
            _ => format!(
                "child agent {child} has not taken its initial prompt yet; it stays queued for the child's next ready window"
            ),
        };
    }
    let state_desc = payload
        .get("state")
        .and_then(|s| s.as_str())
        .unwrap_or("changed");
    let prompt_excerpt = payload
        .get("prompt")
        .and_then(|p| p.as_str())
        .map(|p| {
            let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
            if flat.chars().count() > 120 {
                format!("{}…", flat.chars().take(120).collect::<String>())
            } else {
                flat
            }
        })
        .filter(|p| !p.is_empty());
    match (
        payload.get("exit_code").and_then(|c| c.as_i64()),
        prompt_excerpt,
    ) {
        (Some(code), _) => format!("child agent {child} {state_desc} (exit {code})"),
        (None, Some(prompt)) => format!(
            "child agent {child} is now {state_desc} — answer it with session action=input: {prompt}"
        ),
        (None, None) => format!("child agent {child} is now {state_desc}"),
    }
}

/// Enqueue the authoritative parent lifecycle message without touching the
/// parent's PTY lifecycle lock. BUSY→IDLE and completed paths call this while
/// holding the child's SilenceState transaction lock.
pub(super) fn enqueue_state_change_to_parent(
    state: &AppState,
    session_id: &str,
    payload: serde_json::Value,
) -> Option<ParentLifecycleDispatch> {
    let parent_id = state
        .session_maps
        .session_parent
        .get(session_id)
        .map(|e| e.value().clone())?;
    if parent_id == session_id {
        return None;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let msg = crate::state::AgentMessage {
        id: format!("tuic-auto-{}-{}", session_id, now_ms),
        from_tuic_session: session_id.to_string(),
        from_name: "tuic".to_string(),
        content: serde_json::to_string(&payload).unwrap_or_default(),
        timestamp: now_ms,
        delivered_via_channel: false,
    };
    let message_id = msg.id.clone();
    // DEFERRED (2026-08-05) — this push does not take PEER_IDENTITY_BIND_LOCK, so a
    // lifecycle notice resolved against a parent that is being retired
    // (retire_repaired_phantom_identity) can still land in a drained inbox. The
    // peer-to-peer `send` path was serialized against the retire in story 546-33cb;
    // this one needs the resolution of `parent_id` and the push to share that guard
    // too. Left out of that story's scope deliberately — it needs its own repro,
    // since the parent id here comes from session_parent rather than a caller.
    let message_timestamp = state.push_agent_inbox(&parent_id, msg);
    let framed = format!(
        "[TUIC] {}",
        describe_lifecycle_payload(session_id, &payload)
    );
    let dispatch = ParentLifecycleDispatch {
        parent_id: parent_id.clone(),
        message_id: message_id.clone(),
        message_timestamp,
        framed,
    };
    // Role selection and ownership are deliberately deferred together until
    // after the child lifecycle lock is released. Splitting those decisions
    // allowed a concurrent orchestrator-role removal to create a generic wake
    // and an ordinary payload delivery for the same buffered notification.
    Some(dispatch)
}

/// Wake/dispatch only after the child lifecycle lock has been released. This
/// may acquire the parent's SilenceState lock through terminal delivery.
pub(super) fn dispatch_parent_lifecycle(state: &AppState, dispatch: ParentLifecycleDispatch) {
    if route_registered_orchestrator_mail(
        state,
        &dispatch.parent_id,
        &dispatch.message_id,
        dispatch.message_timestamp,
    )
    .is_some()
    {
        return;
    }
    if state.assign_agent_delivery(
        &dispatch.parent_id,
        &dispatch.message_id,
        state.live_pty_for_peer(&dispatch.parent_id).is_some(),
    ) != crate::state::AgentDeliveryAssignment::Terminal
    {
        return;
    }
    let outcome = deliver_notice_to_managed_pty(state, &dispatch.parent_id, &dispatch.framed);
    settle_terminal_delivery(state, &dispatch.parent_id, &dispatch.message_id, outcome);
}

/// Push a state_change message and wake the parent when no child lifecycle
/// transaction is active (for example, process exit and direct test helpers).
pub(crate) fn push_state_change_to_parent(
    state: &Arc<AppState>,
    session_id: &str,
    payload: serde_json::Value,
) {
    if let Some(dispatch) = enqueue_state_change_to_parent(state, session_id, payload) {
        // The inbox push above already happened on this thread — that is the
        // authoritative copy an `agent wait` can observe. Only the terminal wake
        // is deferred, because it is the part that sleeps `INJECT_ENTER_GAP`, and
        // both live producers here are tokio workers (the session-state
        // accumulator and the reader thread's exit path).
        let state = Arc::clone(state);
        spawn_injection_job(move || dispatch_parent_lifecycle(&state, dispatch));
    }
}

/// Emit the single exceptional-path notification for an initial prompt that has
/// not reached the child's composer yet.
///
/// The prompt is deliberately NOT dropped. A child that stalls on a startup
/// dialog ("Do you trust the contents of this directory?") is not a child whose
/// task is void — once the dialog is answered it reaches a ready prompt and the
/// queued entry is typed. Removing the marker here (as this used to) reported
/// the failure *and* silently discarded the work, so nothing retried and the
/// child sat idle as if it had been spawned with nothing to do.
///
/// `notified` keeps the watchdog one-shot without that loss, and the payload
/// carries both the detected cause and the prompt itself so a parent that
/// prefers to re-deliver by hand has the text.
pub(crate) fn notify_initial_prompt_timeout_if_pending(
    state: &Arc<AppState>,
    session_id: &str,
) -> bool {
    let prompt = {
        let Some(mut pending) = state.pending_initial_prompts.get_mut(session_id) else {
            return false;
        };
        if pending.notified {
            return false;
        }
        pending.notified = true;
        pending.prompt.clone()
    };
    let Some(parent_id) = state
        .session_maps
        .session_parent
        .get(session_id)
        .map(|entry| entry.value().clone())
    else {
        tracing::warn!(session = %session_id, "Initial prompt delivery timed out without a registered parent");
        return false;
    };
    let now_ms = now_epoch_ms();
    // Dialog detection, rather than reporting every stall as a bare timeout: a
    // confident question is the one cause the server can name, and it is the
    // one that resolves by itself the moment a human answers it.
    let reason = if blocked_on_confident_question(state, session_id) {
        "startup_dialog"
    } else {
        "timeout"
    };
    let payload = serde_json::json!({
        "type": "prompt_delivery_failed",
        "reason": reason,
        "session_id": session_id,
        // The prompt is still queued for the child's next ready window; a parent
        // that wants to re-deliver it itself does not have to have kept a copy.
        "retrying": true,
        "prompt": prompt,
    });
    let message_id = format!("tuic-auto-prompt-{session_id}-{now_ms}");
    let message_timestamp = state.push_agent_inbox(
        &parent_id,
        crate::state::AgentMessage {
            id: message_id.clone(),
            from_tuic_session: session_id.to_string(),
            from_name: "tuic".to_string(),
            content: serde_json::to_string(&payload).unwrap_or_default(),
            timestamp: now_ms,
            delivered_via_channel: false,
        },
    );
    if route_registered_orchestrator_mail(state, &parent_id, &message_id, message_timestamp)
        .is_some()
    {
        return true;
    }
    if state.assign_agent_delivery(
        &parent_id,
        &message_id,
        state.session_maps.sessions.contains_key(&parent_id),
    ) != crate::state::AgentDeliveryAssignment::Terminal
    {
        return true;
    }
    // Fired from a tokio watchdog task, so the wake goes to the injection worker.
    let framed = format!(
        "[TUIC] {}",
        describe_lifecycle_payload(session_id, &payload)
    );
    let state = Arc::clone(state);
    spawn_injection_job(move || {
        let outcome = deliver_notice_to_managed_pty(&state, &parent_id, &framed);
        settle_terminal_delivery(&state, &parent_id, &message_id, outcome);
    });
    true
}

/// Tell the parent that a prompt it was warned about has now been typed.
///
/// Only emitted after a `prompt_delivery_failed` notice for the same child: an
/// orchestrator that was told the task never landed must not be left believing
/// that. Silence would be the worse of the two lies, because the only recovery
/// it leaves is re-delivering a prompt that is already running.
fn notify_initial_prompt_delivered(state: &AppState, session_id: &str) {
    if let Some(dispatch) = enqueue_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "prompt_delivered",
            "session_id": session_id,
        }),
    ) {
        dispatch_parent_lifecycle(state, dispatch);
    }
}

/// First 8 chars of a session UUID, for compact human-facing labels.
fn short_session(session_id: &str) -> &str {
    session_id.get(..8).unwrap_or(session_id)
}

/// Whether a framed peer message should be typed into `session_id` right now
/// rather than queued. True only for an agent session that is idle and not
/// blocked on a *confident* user-facing question — writing into a busy Ink TUI
/// can corrupt its render, and writing into a plain shell would execute the
/// message as a command.
///
/// The gate is `question_confident`, NOT `awaiting_input`: agents that idle at
/// a ready prompt (codex) sit permanently at `awaiting_input=true` via the
/// low-confidence silence heuristic, which would starve delivery forever
/// (story 091). Confident questions (Ink footer, cliclack `◆ …?`, "Action
/// Required" titles) still block injection so a peer message never answers a
/// real approval prompt.
pub(super) fn idle_is_confirmed(state: &AppState, session_id: &str) -> bool {
    let confirmed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|sl| sl.lock().idle_confirmed())
        .unwrap_or(false);
    if confirmed {
        return true;
    }
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    // Preserve legacy behavior for agents without a verified ready-screen
    // adapter. Hook-enabled variants become confirmed via explicit Stop; the
    // remaining heuristics cannot yet provide a stronger proof.
    !has_ready_screen_adapter(agent_type.as_deref())
}

/// Whether an agent owns this session's composer. Injection is agent-only: in a
/// plain shell the idle atom says nothing about what holds stdin, so typed text
/// would reach whatever program is running rather than the shell.
fn session_is_agent(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.agent_type.is_some())
        .unwrap_or(false)
}

/// Whether a confident user-facing question currently owns this composer — the
/// startup trust dialog, an approval prompt, an Ink footer choice. Named
/// separately from `should_inject_now` because the prompt-delivery watchdog
/// reports it as a cause, not merely as a reason to wait.
pub(crate) fn blocked_on_confident_question(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.question_confident)
        .unwrap_or(false)
}

pub(crate) fn should_inject_now(state: &AppState, session_id: &str) -> bool {
    submission_ready(state, session_id, false)
}

fn submission_ready(state: &AppState, session_id: &str, human_reply: bool) -> bool {
    if !session_is_agent(state, session_id) {
        return false;
    }
    if state
        .session_maps
        .session_states
        .get(session_id)
        .is_none_or(|session| {
            session.spawn_root_role == crate::state::SpawnRootRole::Unknown
                || session.foreground_input_blocked
        })
    {
        return false;
    }
    let idle = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|a| a.load(std::sync::atomic::Ordering::Relaxed) == SHELL_IDLE)
        .unwrap_or(false);
    let blocked_on_question = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|s| s.question_confident)
        .unwrap_or(false);
    idle && !state
        .session_maps
        .silence_states
        .get(session_id)
        .is_some_and(|silence| silence.lock().injection_delivery_uncertain)
        && idle_is_confirmed(state, session_id)
        && (human_reply || !blocked_on_question)
        && !has_partial_user_input(state, session_id)
}

/// True while the user has characters sitting in the composer. Injecting then
/// would splice our text into what they are typing.
pub(super) fn has_partial_user_input(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .input_buffers
        .get(session_id)
        .is_some_and(|buffer| !buffer.lock().content().is_empty())
}

/// Reserve an idle agent composer for one injected command.
///
/// `should_inject_now` is only a snapshot. The agent may become busy between
/// that read and the PTY write, so the final IDLE→BUSY transition must be an
/// atomic compare-exchange. A lost race leaves the message queued instead of
/// typing it into an active composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InjectionClaim {
    pub(super) token: u64,
    /// The claim moved the atom IDLE→BUSY. A mid-turn voice claim
    /// (`claim_composer_for_voice`) did not, so its rollback must not hand a
    /// working agent an idle edge it never had.
    took_idle: bool,
}

pub(super) fn claim_idle_for_injection(
    state: &AppState,
    session_id: &str,
) -> Option<InjectionClaim> {
    claim_idle_for_submission(state, session_id, false)
}

fn claim_idle_for_submission(
    state: &AppState,
    session_id: &str,
    human_reply: bool,
) -> Option<InjectionClaim> {
    if !submission_ready(state, session_id, human_reply) {
        return None;
    }
    let prior_idle_confirmed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().idle_confirmed())
        .unwrap_or(false);
    if !try_shell_transition(state, session_id, SHELL_IDLE, SHELL_BUSY, true) {
        return None;
    }
    finish_idle_claim(state, session_id, prior_idle_confirmed)
}

/// The half of an idle claim after IDLE→BUSY is ours.
fn finish_idle_claim(
    state: &AppState,
    session_id: &str,
    prior_idle_confirmed: bool,
) -> Option<InjectionClaim> {
    // The composer is re-read after the atom is ours: `should_inject_now` was a
    // snapshot, and the user can start typing in between. Revert before the
    // claim exists so no spurious busy/idle pair reaches the UI.
    if has_partial_user_input(state, session_id) {
        try_shell_transition(state, session_id, SHELL_BUSY, SHELL_IDLE, true);
        return None;
    }
    let token = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().begin_injection_claim(prior_idle_confirmed))
        .unwrap_or(0);
    emit_shell_state(state, session_id, "busy");
    Some(InjectionClaim {
        token,
        took_idle: true,
    })
}

/// Reserve an agent's composer for one hands-free turn, busy or not.
///
/// An idle agent is claimed exactly as the queue claims it, minus the
/// confirmed-idle requirement. A working agent keeps its BUSY atom — it is
/// working — so the claim token alone orders writers: it is refused while
/// another claim is live or an earlier write is uncertain.
///
/// DEFERRED (2026-09-23) — the idle path above does not check for a live
/// mid-turn claim, so if the agent goes idle during a mid-turn voice write, a
/// queue flush can claim IDLE→BUSY and type its entry in the same few ms. The
/// writer mutex keeps the bytes unspliced; only the order of the two
/// submissions can swap. Not worth changing the state machine for now.
fn claim_composer_for_voice(state: &AppState, session_id: &str) -> Option<InjectionClaim> {
    let prior_idle_confirmed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().idle_confirmed())
        .unwrap_or(false);
    if try_shell_transition(state, session_id, SHELL_IDLE, SHELL_BUSY, true) {
        return finish_idle_claim(state, session_id, prior_idle_confirmed);
    }
    let busy = state
        .session_maps
        .shell_states
        .get(session_id)
        .is_some_and(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY);
    if !busy {
        return None;
    }
    let token = state
        .session_maps
        .silence_states
        .get(session_id)?
        .lock()
        .begin_mid_turn_injection_claim()?;
    let claim = InjectionClaim {
        token,
        took_idle: false,
    };
    if has_partial_user_input(state, session_id) {
        rollback_injection_claim(state, session_id, claim);
        return None;
    }
    Some(claim)
}

pub(super) fn rollback_injection_claim(
    state: &AppState,
    session_id: &str,
    claim: InjectionClaim,
) -> bool {
    if !claim.took_idle {
        if let Some(silence) = state.session_maps.silence_states.get(session_id) {
            silence.lock().release_injection_claim(claim.token);
        }
        return false;
    }
    let owns_claim = state
        .session_maps
        .silence_states
        .get(session_id)
        .and_then(|silence| silence.lock().rollback_injection_claim(claim.token))
        .is_some();
    if !owns_claim {
        return false;
    }
    if try_shell_transition(state, session_id, SHELL_BUSY, SHELL_IDLE, true) {
        emit_shell_state(state, session_id, "idle");
        true
    } else {
        false
    }
}

pub(super) fn mark_injection_uncertain(state: &AppState, session_id: &str, claim: InjectionClaim) {
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        silence.lock().mark_injection_uncertain(claim.token);
    }
}

pub(super) fn mark_orchestrator_notice_uncertain(
    state: &AppState,
    session_id: &str,
    claim: InjectionClaim,
) {
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        silence
            .lock()
            .mark_orchestrator_notice_uncertain(claim.token);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AgentSubmissionWrite {
    Complete {
        acknowledgement_offset: u64,
    },
    Rejected {
        reason: &'static str,
        composer_state: &'static str,
        /// What is parked ahead of this submission, for `queued_commands_pending`.
        /// Empty for every other reason.
        pending: Vec<PendingInjectionSummary>,
    },
    Failed(String),
    Uncertain(String),
}

/// One parked entry, as a rejected submission reports it.
///
/// `queued_commands_pending` used to be a bare string against a `queued_commands`
/// count that deliberately excluded server entries, so a caller looking at an
/// empty composer and an empty Compose queue had nothing to act on. The blocker
/// now names itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PendingInjectionSummary {
    pub id: u64,
    pub kind: &'static str,
    /// First line, truncated. Enough to recognise the entry; the full text is on
    /// the Compose queue listing.
    pub preview: String,
}

const PENDING_PREVIEW_MAX_CHARS: usize = 80;

fn summarize_pending_injections(
    state: &AppState,
    session_id: &str,
) -> Vec<PendingInjectionSummary> {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| {
            queue
                .iter()
                .map(|entry| PendingInjectionSummary {
                    id: entry.id(),
                    kind: entry.kind(),
                    preview: truncate_chars(
                        entry.text().lines().next().unwrap_or_default(),
                        PENDING_PREVIEW_MAX_CHARS,
                    ),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>())
}

fn agent_composer_rejection(
    state: &AppState,
    session_id: &str,
    human_reply: bool,
) -> Option<(&'static str, &'static str)> {
    if !state.session_maps.sessions.contains_key(session_id) {
        return Some(("session_not_found", "unknown"));
    }
    if !session_is_agent(state, session_id) {
        return Some(("not_managed_agent", "unknown"));
    }
    if has_partial_user_input(state, session_id) {
        return Some(("partial_composer", "partial"));
    }
    if !human_reply
        && state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.question_confident)
    {
        return Some(("awaiting_input", "empty"));
    }
    if !submission_ready(state, session_id, human_reply) {
        return Some(("agent_not_ready", "empty"));
    }
    None
}

/// Explain a safe submission refusal without changing delivery or composer state.
pub(crate) fn agent_submission_rejection_detail(
    state: &AppState,
    session_id: &str,
    reason: &str,
) -> String {
    match reason {
        "session_not_found" => "The PTY session is closed or unknown. List sessions and choose a live terminal.".into(),
        "observation_unavailable" => "The PTY output observer is not available, so submission cannot be acknowledged. Wait for session initialization or choose a live terminal.".into(),
        "not_managed_agent" => {
            let foreground = session_leaf_pid(state, session_id)
                .and_then(process_name_from_pid)
                .unwrap_or_else(|| "unknown (process lookup unavailable)".into());
            format!("No supported agent has been detected in this PTY; foreground process: {foreground}. Start a supported agent and wait for backend detection, or launch a custom wrapper through an agent run configuration. TUIC_SESSION identifies the terminal, not an agent.")
        }
        "partial_composer" => "The composer contains unfinished user input. Submit or clear that input before sending another command.".into(),
        "awaiting_input" => "An approval or question owns the composer. Have the user answer it, or use an explicit human reply; automated submit must wait.".into(),
        "agent_not_ready" => {
            if state.session_maps.session_states.get(session_id).is_some_and(|session| {
                session.spawn_root_role == crate::state::SpawnRootRole::Unknown || session.foreground_input_blocked
            }) {
                return "The owning process cannot accept unattended input: its root role or foreground is unavailable, or a direct agent child owns the terminal. Wait for the agent to regain foreground or use its authoritative daemon connection.".into();
            }

            if state.session_maps.silence_states.get(session_id)
                .is_some_and(|silence| silence.lock().injection_delivery_uncertain)
            {
                "A previous PTY write has an uncertain outcome. Inspect terminal output and wait for confirmed readiness; do not resend text that may already have reached the agent.".into()
            } else if state.session_maps.shell_states.get(session_id)
                .is_some_and(|shell| shell.load(Ordering::Acquire) == SHELL_BUSY)
            {
                "The agent is still busy or starting. Wait for its idle composer or Stop signal before submitting.".into()
            } else {
                "The backend has not confirmed a ready agent composer. Bring the normal prompt into view or wait for an agent idle hook; then retry.".into()
            }
        }
        "queued_commands_pending" => "Earlier commands still own the delivery queue. Wait for them to drain or inspect and remove unwanted queued entries before submitting.".into(),
        "claim_lost" => "The composer changed while submission was claiming it. Wait for confirmed readiness and retry this unwritten command.".into(),
        _ => "The terminal could not safely accept input. Inspect its state and output before retrying.".into(),
    }
}

/// Describe why a terminal could not receive an inbox wake; never drains a queue.
pub(crate) fn agent_mail_wake_detail(state: &AppState, session_id: &str) -> String {
    let reason = agent_composer_rejection(state, session_id, false)
        .map(|(reason, _)| agent_submission_rejection_detail(state, session_id, reason))
        .unwrap_or_else(|| "The terminal wake path is unavailable. Inspect the recipient's lifecycle before retrying.".into());
    format!(
        "{reason} Mail remains in the inbox; the recipient can read it with agent action=inbox or wait."
    )
}

pub(super) fn agent_submission_rejection(
    state: &AppState,
    session_id: &str,
    human_reply: bool,
) -> Option<(&'static str, &'static str)> {
    if let Some(rejection) = agent_composer_rejection(state, session_id, human_reply) {
        return Some(rejection);
    }
    // Readiness is checked BEFORE the queue, and the order is the fix. An agent
    // whose idle is unconfirmed can never drain its queue — `flush_pending_injections`
    // is gated on the same predicate — so reporting `queued_commands_pending`
    // named the symptom and hid the cause, and the caller retried submit for
    // minutes against a queue that by construction could not move.
    // `agent_not_ready` is the truth, and it is the state that actually changes.
    // A confident question belongs to the human. Its parked automated entries
    // cannot drain until the answer clears the question, and must not prevent
    // that answer from reaching the composer.
    if !(human_reply && blocked_on_confident_question(state, session_id)) {
        // The agent IS ready, so anything still parked can be typed right now.
        // Level-triggered: the BUSY→IDLE edge that normally drains this queue may
        // already have passed, and nothing else would fire it. Draining here is the
        // same write the transition would have made, one item per idle window.
        if state
            .pending_injections
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            flush_pending_injections_blocking(state, session_id);
        }
        if state
            .pending_injections
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            return Some(("queued_commands_pending", "empty"));
        }
    }
    // Re-read: a flush that emptied the queue typed one entry and left the
    // session BUSY, so the caller is now waiting on that turn, not on a queue.
    if !submission_ready(state, session_id, human_reply) {
        return Some(("agent_not_ready", "empty"));
    }
    None
}

/// Claim and write one MCP-managed agent command without queueing it.
///
/// This is the ordering half of `session action=submit`. The caller owns the
/// existing input-bookkeeping FSM and the bounded acknowledgement wait after a
/// complete write. Rejections happen before the first byte. Once the claim is
/// held, concurrent peer delivery observes BUSY and queues behind this command.
pub(crate) fn write_agent_submission_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> AgentSubmissionWrite {
    write_submission_to_pty(state, session_id, text, false)
}

/// An explicit human answer may claim a confident question's composer; an
/// automated agent submission continues to wait for that question to clear.
pub(crate) fn write_human_reply_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> AgentSubmissionWrite {
    write_submission_to_pty(state, session_id, text, true)
}

fn write_submission_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
    human_reply: bool,
) -> AgentSubmissionWrite {
    if let Some((reason, composer_state)) =
        agent_submission_rejection(state, session_id, human_reply)
    {
        return AgentSubmissionWrite::Rejected {
            reason,
            composer_state,
            pending: summarize_pending_injections(state, session_id),
        };
    }
    let Some(claim) = claim_idle_for_submission(state, session_id, human_reply) else {
        let (reason, composer_state) = agent_submission_rejection(state, session_id, human_reply)
            .unwrap_or(("claim_lost", "unknown"));
        return AgentSubmissionWrite::Rejected {
            reason,
            composer_state,
            pending: summarize_pending_injections(state, session_id),
        };
    };

    let (outcome, acknowledgement_offset) =
        write_agent_command_with_boundary(state, session_id, text);
    match outcome {
        InjectionOutcome::Submitted => {
            // Terminal movement during the split write can invalidate the claim;
            // that is independent evidence, not a reason to discard a completed
            // write. Clear the token when it is still ours. The MCP caller advances
            // the turn through InputLineBuffer exactly once.
            if let Some(silence) = state.session_maps.silence_states.get(session_id) {
                silence.lock().commit_injection_claim(claim.token);
            }
            AgentSubmissionWrite::Complete {
                acknowledgement_offset,
            }
        }
        InjectionOutcome::NotStarted(error) => {
            rollback_injection_claim(state, session_id, claim);
            AgentSubmissionWrite::Failed(error)
        }
        InjectionOutcome::Uncertain(error) => {
            mark_injection_uncertain(state, session_id, claim);
            AgentSubmissionWrite::Uncertain(error)
        }
    }
}

/// Build the text write of an injection: multiline text rides inside a
/// bracketed paste (ESC[200~ … ESC[201~) so the TUI keeps embedded newlines as
/// paste content and the trailing CR (sent as a separate write) lands as a real
/// Enter keypress. Mirrors the frontend `sendCommand.ts` recipe exactly — raw
/// multiline text merely PREFILLS codex/claude without submitting (verified
/// live, story 091).
///
/// The Ctrl-U that clears pending input is NOT part of it: it goes out
/// `INJECT_ENTER_GAP` earlier, in its own read. Claude Code (verified live on
/// v2.1.280) treats a long chunk as a paste, strips a Ctrl-U inside it as an
/// invisible character and then refuses the Enter — a 584-char submission
/// stayed unsent even with a 500ms Enter gap.
pub(super) fn injection_payload(text: &str) -> String {
    if text.contains('\n') {
        bracketed_payload(text)
    } else {
        text.to_string()
    }
}

fn bracketed_payload(text: &str) -> String {
    format!("\x1b[200~{text}\x1b[201~")
}

/// Single-line length above which Codex gets a bracketed paste: see
/// `agent_submit_profile`. Measured: 600 chars submit at a 200ms gap, 1000 do
/// not; the margin below 600 absorbs a slower machine. Shorter text stays plain
/// keystrokes, so slash commands and one-key answers keep working.
const CODEX_PASTE_FRAME_MIN_CHARS: usize = 500;

fn codex_payload(text: &str) -> String {
    if text.chars().count() > CODEX_PASTE_FRAME_MIN_CHARS {
        bracketed_payload(text)
    } else {
        injection_payload(text)
    }
}

/// Paste into an agent composer without clearing or submitting existing input.
pub(crate) fn prefill_agent_input(
    state: &Arc<AppState>,
    session_id: &str,
    text: &str,
) -> Result<(), String> {
    if !session_is_agent(state, session_id) {
        return Err("Session is not running an agent".into());
    }
    crate::mcp_http::session::write_pty_input_parts(
        state,
        session_id,
        &["\x1b[200~", text, "\x1b[201~"],
    )
}

/// Real-time gap inserted between the payload write and the Enter write of an
/// injection. Ink/raw-mode agents (Codex, Claude Code) only treat the trailing
/// CR as a submit when it arrives in a SEPARATE `read()` from the text; a
/// microsecond-apart back-to-back write — even with a flush in between — is
/// coalesced into one read and the CR is swallowed as part of the typed buffer,
/// so the message just sits at the prompt unsubmitted (verified live against
/// Claude Code 2.1.286: plain text + CR with no gap stays in the composer
/// with "Removed 1 invisible character"; 50ms submits).
/// 50ms clears the child's read-scheduling latency for every verified agent.
/// An agent TUICommander cannot identify, or has not verified, keeps a 200ms gap
/// and the plain payload; Ctrl-U is a control key before those characters.
///
/// This comment used to claim the frontend `sendCommand.ts` recipe "gets this
/// gap for free — its two `writeFn` calls are separate IPC round-trips". It does
/// NOT: a Tauri IPC round-trip completes well inside the child's read latency,
/// so both writes land in one `read()` and a clicked suggestion renders as a
/// newline instead of submitting. `sendCommand.ts` waits 50ms after Ctrl-U
/// and before non-Codex Enter. Keep both frontend
/// timing rules in step — separate flushes never guaranteed separate reads.
pub(super) const INJECT_ENTER_GAP: std::time::Duration = std::time::Duration::from_millis(50);
const UNVERIFIED_ENTER_GAP: std::time::Duration = std::time::Duration::from_millis(200);
// A Codex stop hook delayed Working by four seconds. A live Claude notice took
// 3.8 seconds to leave its internal queue after Enter. Allow both delayed
// agents to publish a positive signal before reporting uncertain delivery.
const DELAYED_AGENT_QUEUED_SUBMISSION_CONFIRMATION: std::time::Duration =
    std::time::Duration::from_secs(6);

/// Per-agent submit recipe: payload form, Enter gap and the signal that
/// confirms the turn started. One table for every injection path.
///
/// Measured live 2026-10-02 in a PTY driven with the exact injection bytes
/// (Ctrl-U, 50ms, payload, gap, CR). `ok` = the turn started on the first Enter.
///
/// | agent    | version | plain, short (<= 200 chars) | plain, 1000-2000 chars | bracketed paste |
/// |----------|---------|-----------------------------|------------------------|-----------------|
/// | codex    | 0.159.0 | ok at 0/50/200ms | Enter SWALLOWED at 200ms (1000, 1400, 2000 chars); ok at 600ms gap | ok at 0ms (190, 1000 chars) and 50ms (2000) |
/// | claude   | 2.1.286 | CR swallowed at 0ms ("Removed 1 invisible character"); ok at 50ms | ok at 50ms | ok at 0/50ms |
/// | grok     | 1.0.44  | ok at 0ms | ok at 50ms | ok at 0/50ms |
/// | pi       | 0.84.2  | ok at 0ms | ok at 50ms | ok at 0/50ms |
/// | opencode | 1.18.30 | ok at 50ms | ok at 50ms | ok at 50ms |
/// | goose    | 1.49.0  | ok at 50ms | ok at 50ms | ok at 50ms |
/// | gemini, aider, amp, cursor, droid | not installed: UNVERIFIED |
///
/// Ground truth of the probe was an answer token that exists only if the turn
/// ran; the `confirmation` signals below were NOT re-measured against the screen
/// classifiers (the adapters keep their own captured fixtures). opencode needs
/// `OPENCODE_DISABLE_AUTOUPDATE=true` under such a probe: its "Update Available"
/// modal turns an injected Enter into "Confirm".
///
/// Codex consumes a long plain write as a paste burst and keeps ingesting for
/// hundreds of milliseconds (a 1967-char brief took ~680ms in the real capture
/// `codex-0.159-long-brief-swallowed-enter.tcap`); an Enter that lands inside
/// that burst becomes part of the paste and the text stays in the composer.
/// A fixed gap cannot cover a payload whose ingestion time grows with its
/// length, so a Codex payload over `CODEX_PASTE_FRAME_MIN_CHARS` (or multiline)
/// is a bracketed paste: Codex takes it as one paste event and the later CR is
/// an ordinary Enter. Short text stays plain: Codex's approval and choice
/// overlays act on key presses and ignore a paste event.
///
/// The frontend's `sendCommand.ts` keeps the same table for user-originated
/// PTY writes.
#[derive(Clone, Copy)]
pub(super) struct AgentSubmitProfile {
    pub(super) payload: fn(&str) -> String,
    pub(super) enter_gap: std::time::Duration,
    confirmation: SubmitConfirmation,
}

#[derive(Clone, Copy)]
enum SubmitConfirmation {
    WorkingScreen,
    PromptGone,
    HookOnly,
    LegacyWrite,
}

pub(super) fn agent_submit_profile(agent_type: Option<&str>) -> AgentSubmitProfile {
    use SubmitConfirmation::{HookOnly, LegacyWrite, PromptGone, WorkingScreen};
    let (enter_gap, confirmation) = match agent_type {
        Some("codex") => (INJECT_ENTER_GAP, WorkingScreen),
        Some("claude" | "opencode" | "goose" | "grok" | "pi") => (INJECT_ENTER_GAP, WorkingScreen),
        Some("gemini" | "aider") => (INJECT_ENTER_GAP, PromptGone),
        Some("amp" | "cursor" | "droid") => (INJECT_ENTER_GAP, LegacyWrite),
        _ => (UNVERIFIED_ENTER_GAP, HookOnly),
    };
    AgentSubmitProfile {
        payload: if agent_type == Some("codex") {
            codex_payload
        } else {
            injection_payload
        },
        enter_gap,
        confirmation,
    }
}

pub(super) fn injection_enter_gap(agent_type: Option<&str>) -> std::time::Duration {
    agent_submit_profile(agent_type).enter_gap
}

pub(crate) fn sleep_agent_enter_gap(agent_type: Option<&str>) {
    std::thread::sleep(injection_enter_gap(agent_type));
}

/// One piece of injection work, handed off by a caller that must not block.
type InjectionJob = Box<dyn FnOnce() + Send + 'static>;

/// The thread that pays `INJECT_ENTER_GAP` so tokio workers do not.
///
/// Three producers reach injection from a tokio worker — the session-state
/// accumulator, the per-session silence timer, and the `agent wait` guard's
/// `Drop` — and each would park that worker for two gaps (~100ms: one before
/// the text, one before the Enter) per message. They enqueue here instead.
///
/// ONE thread, not one per job, and that is the whole design: lifecycle
/// notifications for a parent (`idle` → `completed` → `exited`) must reach its
/// composer in the order they were produced, and a thread per job would let
/// `exited` overtake `idle`. A single FIFO consumer preserves the ordering the
/// callers used to get for free by being synchronous.
///
/// The channel is unbounded on purpose: a bounded one could block the very
/// caller this exists to unblock, and a job may enqueue more work re-entrantly
/// (a delivery that transitions a session re-runs the flush).
static INJECTION_QUEUE: LazyLock<std::sync::mpsc::Sender<InjectionJob>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel::<InjectionJob>();
    std::thread::Builder::new()
        .name("tuic-injection".to_string())
        .spawn(move || {
            for job in rx {
                job();
            }
        })
        .expect("injection worker thread");
    tx
});

/// Block until every injection enqueued before this call has run.
///
/// The worker is FIFO, so a job that signals us cannot run ahead of the ones
/// queued before it. Tests asserting on the *result* of a detached injection
/// need this; polling for the effect instead turns each such assertion into a
/// timing race that reports a scheduling delay as a delivery bug.
#[cfg(test)]
pub(crate) fn wait_for_injection_queue() {
    let (tx, rx) = std::sync::mpsc::channel();
    spawn_injection_job(move || {
        let _ = tx.send(());
    });
    rx.recv().expect("injection worker must drain");
}

/// Run `job` on the injection worker instead of the calling thread.
pub(crate) fn spawn_injection_job(job: impl FnOnce() + Send + 'static) {
    // The receiver lives for the process, so this can only fail if the worker
    // panicked. Running the job inline would reintroduce exactly the block this
    // exists to remove, so report the loss instead of hiding it in a stall.
    if INJECTION_QUEUE.send(Box::new(job)).is_err() {
        tracing::error!("injection worker is gone; a queued injection was dropped");
    }
}

/// Write prompt text and a submitting Enter to an agent PTY using the exact
/// framing and timing required by raw-mode TUIs. The caller owns bookkeeping:
/// peer delivery records a synthetic submission, while MCP session input feeds
/// the original text and Enter through its input-state FSM. One writer guard
/// spans the real scheduling gap, so another producer cannot splice the line.
pub(crate) fn write_agent_command_to_pty(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> Result<(), String> {
    match write_agent_command_with_boundary(state, session_id, text).0 {
        InjectionOutcome::Submitted => Ok(()),
        InjectionOutcome::NotStarted(error) | InjectionOutcome::Uncertain(error) => Err(error),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum InjectionOutcome {
    Submitted,
    NotStarted(String),
    Uncertain(String),
}

pub(super) fn write_all_with_progress(
    writer: &mut dyn Write,
    bytes: &[u8],
    prior_bytes_written: usize,
) -> Result<(), (usize, String)> {
    let mut written = 0usize;
    while written < bytes.len() {
        match writer.write(&bytes[written..]) {
            Ok(0) => {
                return Err((
                    prior_bytes_written + written,
                    "Write failed: writer returned zero bytes".to_string(),
                ));
            }
            Ok(n) => written += n,
            Err(error) => {
                return Err((
                    prior_bytes_written + written,
                    format!("Write failed: {error}"),
                ));
            }
        }
    }
    Ok(())
}

fn write_agent_command_with_boundary(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> (InjectionOutcome, u64) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let profile = agent_submit_profile(agent_type.as_deref());
    let payload = (profile.payload)(text);
    let writer = match state.pty_writer(session_id) {
        Some(writer) => writer,
        None => {
            return (
                InjectionOutcome::NotStarted("Session not found".to_string()),
                0,
            );
        }
    };
    // One writer guard spans payload, scheduling gap, and Enter. The injection
    // claim orders managed peers; this mutex also keeps raw/UI writers from
    // splicing bytes into the command while the child is allowed to consume the
    // payload as a separate read.
    let mut writer = writer.lock();
    state.retire_turn_interrupt(session_id);
    // Ctrl-U first, alone: see `injection_payload`. It types nothing, so a
    // failure before the first text byte cannot make a retry type the command
    // twice — hence it reports `NotStarted` and the text write counts from zero.
    if let Err((_, error)) = write_all_with_progress(writer.as_mut(), b"\x15", 0) {
        return (InjectionOutcome::NotStarted(error), 0);
    }
    if let Err(error) = writer.flush() {
        return (
            InjectionOutcome::NotStarted(format!("Flush failed: {error}")),
            0,
        );
    }
    std::thread::sleep(INJECT_ENTER_GAP);
    if let Err((written, error)) = write_all_with_progress(writer.as_mut(), payload.as_bytes(), 0) {
        return (
            if written == 0 {
                InjectionOutcome::NotStarted(error)
            } else {
                InjectionOutcome::Uncertain(error)
            },
            0,
        );
    }
    if let Err(error) = writer.flush() {
        return (
            InjectionOutcome::Uncertain(format!("Flush failed: {error}")),
            0,
        );
    }

    // Blocks the calling thread, under the writer guard, for the whole gap. Both
    // properties are load-bearing and neither is negotiable here: the child only
    // reads the CR as a submit when it arrives in a separate `read()`, and
    // `agent_submission_writer_lock_prevents_raw_input_splicing` pins the byte
    // sequence this guard protects. A caller that must not block therefore does
    // not shorten the gap — it stops being the thread that waits, by handing the
    // whole sequence to `INJECTION_QUEUE`.
    std::thread::sleep(profile.enter_gap);

    // Exclude payload echo already observable before Enter. The async handler
    // checks this boundary only after the complete Enter write returns; movement
    // beyond it is child PTY output, never TUICommander's own turn bookkeeping.
    let acknowledgement_offset = state
        .session_maps
        .output_buffers
        .get(session_id)
        .map(|buffer| buffer.lock().total_written)
        .unwrap_or(0);
    if let Err((_, error)) = write_all_with_progress(writer.as_mut(), b"\r", payload.len()) {
        return (InjectionOutcome::Uncertain(error), acknowledgement_offset);
    }
    if let Err(error) = writer.flush() {
        return (
            InjectionOutcome::Uncertain(format!("Flush failed: {error}")),
            acknowledgement_offset,
        );
    }

    (InjectionOutcome::Submitted, acknowledgement_offset)
}

/// The submit rule for flush and MCP submit (queued input, brief): write, wait
/// for the child to confirm the turn started, and send at most one more Enter
/// when the composer still holds the text. Lifecycle notices and voice are
/// write-only and never reach this function. Returns the outcome and how the
/// retry went (`none`, `confirmed`, `unconfirmed`).
fn submit_and_confirm(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> (InjectionOutcome, &'static str) {
    let initial_screen = agent_submission_ack_kind(state, session_id);
    let legacy_write = state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| {
            matches!(
                agent_submit_profile(session.agent_type.as_deref()).confirmation,
                SubmitConfirmation::LegacyWrite
            )
        });
    let (write_outcome, acknowledgement_offset) =
        write_agent_command_with_boundary(state, session_id, text);
    let mut enter_retry = "none";
    let outcome = if write_outcome == InjectionOutcome::Submitted
        && !legacy_write
        && !wait_for_queued_submission(state, session_id, acknowledgement_offset, initial_screen)
    {
        if retry_enter_for_retained_composer(state, session_id, text, initial_screen) {
            enter_retry = "confirmed";
            InjectionOutcome::Submitted
        } else {
            if composer_retains_text(state, session_id, text) {
                enter_retry = "unconfirmed";
            }
            InjectionOutcome::Uncertain(
                "Enter was written, but agent submission was not confirmed".into(),
            )
        }
    } else {
        write_outcome
    };
    (outcome, enter_retry)
}

pub(super) fn commit_injection_claim(state: &AppState, session_id: &str, claim: InjectionClaim) {
    let committed = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|silence| silence.lock().commit_injection_claim(claim.token))
        .unwrap_or(false);
    if committed {
        note_submitted_input(state, session_id);
    }
}

/// What an ambiguous write is allowed to do next.
///
/// Retrying a peer message risks typing it twice; an orchestrator notice is
/// either payload-free or a re-derivable state summary, so it is idempotent
/// enough to retry. This used to be inferred by comparing the text against
/// `PEER_MAIL_WAKE` — which silently stopped covering the notice once
/// it could also be a lifecycle summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClaimedInjectionKind {
    Message,
    OrchestratorNotice,
}

fn run_claimed_injection(
    state: &AppState,
    session_id: &str,
    text: &str,
    claim: InjectionClaim,
    kind: ClaimedInjectionKind,
) -> InjectionOutcome {
    // Write-only on purpose: notices (lifecycle, wake, mail, urgent) and voice
    // turns run on the single FIFO `tuic-injection` worker or a caller that must
    // not stall, and a silent agent would hold it for the whole confirmation
    // window. Confirmation belongs to `flush_pending_injections_blocking` and
    // MCP submit.
    let outcome = write_agent_command_with_boundary(state, session_id, text).0;
    apply_claimed_injection_outcome(state, session_id, text, claim, outcome, kind)
}

/// The only thing a peer `send` is ever allowed to put on a recipient's screen.
///
/// It is a pointer, not the message. Typing the payload itself was the whole
/// defect: a recipient's TUI cannot tell an injected line from something its
/// user typed, so mail arrived as keystrokes in the composer — rendered as
/// literal prompt text by one agent, and left sitting unsubmitted (composer
/// "partial") by another. The inbox is where mail lives; this line only tells
/// the recipient to go read it.
pub(crate) const PEER_MAIL_WAKE: &str =
    "[TUIC] message available — read it with: agent action=inbox";

/// Submit an inbox pointer into a busy Claude Code or Codex composer. Their
/// 2026-09-27 live probes show that Enter queues the line for the next tool
/// boundary; Ctrl+Enter and Escape interrupt active work and are not used.
/// The peer's message body never enters this writer or the pending PTY queue.
pub(crate) fn deliver_urgent_mail_notice(
    state: &AppState,
    session_id: &str,
    sender_identity: &str,
) -> Result<(), &'static str> {
    if !state.session_maps.sessions.contains_key(session_id) {
        return Err("recipient_exited");
    }
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    if !matches!(agent_type.as_deref(), Some("claude" | "codex")) {
        return Err("unknown_agent_type");
    }
    if blocked_on_confident_question(state, session_id) {
        return Err("dialog_open");
    }
    if has_partial_user_input(state, session_id) {
        return Err("composer_has_user_text");
    }
    let shell_busy = state
        .session_maps
        .shell_states
        .get(session_id)
        .is_some_and(|shell| shell.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY);
    if shell_busy {
        if state
            .session_state_with_shell(session_id)
            .is_none_or(|session| session.agent_state.as_deref() != Some("working"))
        {
            return Err("recipient_not_ready");
        }
    } else if !should_inject_now(state, session_id) {
        return Err("recipient_not_ready");
    }
    // Display names are peer-controlled prompt text. Use only a validated
    // identity in the notice; the inbox keeps the human-facing sender name.
    let sender_id = uuid::Uuid::parse_str(sender_identity)
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "unknown-peer".to_string());
    let notice = format!(
        "URGENT mail from {sender_id}: read agent inbox before your next step (agent action=inbox)"
    );
    let Some(claim) = claim_composer_for_voice(state, session_id) else {
        return Err(if has_partial_user_input(state, session_id) {
            "composer_has_user_text"
        } else {
            "composer_in_flight"
        });
    };
    if blocked_on_confident_question(state, session_id) {
        rollback_injection_claim(state, session_id, claim);
        return Err("dialog_open");
    }
    match run_claimed_injection(
        state,
        session_id,
        &notice,
        claim,
        ClaimedInjectionKind::Message,
    ) {
        InjectionOutcome::Submitted => Ok(()),
        InjectionOutcome::NotStarted(_) => Err("write_not_started"),
        InjectionOutcome::Uncertain(_) => Err("write_uncertain"),
    }
}

/// Longest self-acknowledging summary we are willing to type into a composer.
/// Past this the notice stops being a cheap one-liner, so we fall back to the
/// generic wake — which is always correct, just one `inbox` call more expensive.
const ORCHESTRATOR_SUMMARY_MAX_CHARS: usize = 240;

/// Render the reserved wake group as a self-contained notice, or `None` when
/// the recipient must be sent to its inbox instead.
///
/// Why this exists: an orchestrator's inbox is dominated by server-authored
/// lifecycle notifications (`idle`, `completed`, `exited`) whose entire payload
/// is a state name. Making the orchestrator spend a tool call to discover
/// "child 8c26 went idle, then exited(0)" is pure round-trip with no
/// information gain, and it happens once per finished child.
///
/// Why it is conditional — the group must be lifecycle-only:
///   1. Peer `send` payloads are agent-authored, arbitrary length and
///      arbitrary content. They stay out of the composer, full stop (that is
///      the invariant `assign_orchestrator_delivery_with_wake_outcome` exists
///      to protect).
///   2. A partial summary would be worse than none: the recipient, satisfied
///      by what it read, would never call `inbox` and would silently lose the
///      messages the summary omitted. So a single non-lifecycle message in the
///      window disqualifies the whole group rather than being skipped.
pub(super) fn summarize_lifecycle_group(
    state: &AppState,
    recipient: &str,
    group: crate::state::OrchestratorWakeGroup,
) -> Option<String> {
    let mut parts = Vec::new();
    {
        let inbox = state.agent_inbox.get(recipient)?;
        for message in inbox.iter().filter(|message| {
            message.timestamp > group.observed_through && message.timestamp <= group.wake_through
        }) {
            if !message
                .id
                .starts_with(crate::state::LIFECYCLE_MSG_ID_PREFIX)
            {
                return None;
            }
            let payload = serde_json::from_str::<serde_json::Value>(&message.content).ok()?;
            parts.push(describe_lifecycle_payload(
                &message.from_tuic_session,
                &payload,
            ));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let summary = format!("[TUIC] {}", parts.join("; "));
    (summary.chars().count() <= ORCHESTRATOR_SUMMARY_MAX_CHARS).then_some(summary)
}

/// Whether a managed agent may safely receive a new, payload-free mail turn.
///
/// Canonical idle/completed lifecycle remains sufficient. A derived `working`
/// state can also be safe when it comes only from background work: in that
/// case the stricter composer gate proves the shell is idle, readiness is
/// confirmed, and neither a question nor partial input owns the composer.
/// Other lifecycle states fail closed.
pub(crate) fn managed_mail_wake_allowed(state: &AppState, session_id: &str) -> bool {
    let Some(session) = state.session_state_with_shell(session_id) else {
        return false;
    };
    match session.agent_state.as_deref() {
        Some("idle" | "completed") => true,
        Some("working") => {
            session.background_work
                && !session.has_pending_background_probe()
                && should_inject_now(state, session_id)
        }
        _ => false,
    }
}

/// Submit one notification only when the registered parent's lifecycle or
/// confirmed-ready composer says that a new turn is safe. Unlike ordinary
/// managed-peer delivery, a lost readiness race is never queued.
///
/// The line is either a self-acknowledging lifecycle summary (see
/// `summarize_lifecycle_group`) or the payload-free generic wake.
fn submit_orchestrator_mail_wake(
    state: &AppState,
    session_id: &str,
    recipient: &str,
    group: crate::state::OrchestratorWakeGroup,
) -> crate::state::OrchestratorWakeAttemptOutcome {
    use crate::state::OrchestratorWakeAttemptOutcome;

    if !managed_mail_wake_allowed(state, session_id) {
        return OrchestratorWakeAttemptOutcome::NotStarted;
    }
    #[cfg(unix)]
    if let Err(error) = wake_session(state, session_id) {
        tracing::debug!(session = %session_id, error, "Orchestrator mail wake failed");
    }
    let Some(claim) = claim_idle_for_injection(state, session_id) else {
        return OrchestratorWakeAttemptOutcome::NotStarted;
    };
    let summary = summarize_lifecycle_group(state, recipient, group);
    let text = summary.as_deref().unwrap_or(PEER_MAIL_WAKE);
    match run_claimed_injection(
        state,
        session_id,
        text,
        claim,
        ClaimedInjectionKind::OrchestratorNotice,
    ) {
        // An uncertain write must NOT acknowledge: the cursor may only advance
        // behind a line we know reached the composer.
        InjectionOutcome::Submitted if summary.is_some() => {
            OrchestratorWakeAttemptOutcome::SummarySubmitted
        }
        InjectionOutcome::Submitted => OrchestratorWakeAttemptOutcome::Submitted,
        InjectionOutcome::NotStarted(_) => OrchestratorWakeAttemptOutcome::NotStarted,
        InjectionOutcome::Uncertain(_) => OrchestratorWakeAttemptOutcome::Uncertain,
    }
}

/// Route mail for a peer that has authoritatively acted as an orchestrator by
/// spawning a managed child: coalesced wakes, a self-acknowledging lifecycle
/// summary where the window allows one, and a strict no-queue policy.
///
/// Returns `None` for ordinary managed agents, which take the simpler
/// SSE-channel-or-`PEER_MAIL_WAKE` route. Both keep the same invariant — the
/// payload never reaches a composer — so what differs is only how the wake is
/// batched, not what a recipient may be shown.
pub(crate) fn route_registered_orchestrator_mail(
    state: &AppState,
    recipient: &str,
    message_id: &str,
    message_timestamp: u64,
) -> Option<crate::state::OrchestratorDeliveryAssignment> {
    if !state.orchestrator_peers.contains(recipient) {
        return None;
    }
    let pty_session = state.live_pty_for_peer(recipient);
    let wake_allowed = pty_session
        .as_deref()
        .is_some_and(|session_id| managed_mail_wake_allowed(state, session_id));
    let assignment = state.assign_orchestrator_delivery_with_wake_outcome(
        recipient,
        message_id,
        message_timestamp,
        wake_allowed,
        |group| match pty_session.as_deref() {
            Some(session_id) => submit_orchestrator_mail_wake(state, session_id, recipient, group),
            None => crate::state::OrchestratorWakeAttemptOutcome::NotStarted,
        },
    );
    // A self-acknowledging notice covers only the window it reserved. Mail that
    // landed while it was being typed keeps `orchestrator_wake_needed_through`
    // set, and nothing else would surface it: the idle/completed transition that
    // normally drives `reevaluate_orchestrator_mail_wake` has already happened.
    // Chase it here instead. Bounded: each pass spends one of
    // ORCHESTRATOR_WAKE_ATTEMPT_LIMIT attempts, and no budget is granted inside
    // one delivery, so the recursion stops at the limit.
    if assignment == crate::state::OrchestratorDeliveryAssignment::WakeSummarySubmitted
        && pty_session.is_some()
    {
        chase_orchestrator_mail_wake(state, recipient);
    }
    Some(assignment)
}

/// Retry buffered orchestrator mail when the managed PTY has reached a
/// canonical idle/completed lifecycle. Busy and unknown states remain inbox-only.
pub(super) fn orchestrator_recipient_for_pty(
    state: &AppState,
    pty_session: &str,
) -> Option<String> {
    if state.orchestrator_peers.contains(pty_session) {
        Some(pty_session.to_string())
    } else {
        state.orchestrator_peers.iter().find_map(|peer| {
            let peer_id = peer.key();
            (state.live_pty_for_peer(peer_id).as_deref() == Some(pty_session))
                .then(|| peer_id.clone())
        })
    }
}

pub(super) fn reevaluate_orchestrator_mail_wake(state: &AppState, pty_session: &str) {
    let Some(recipient) = orchestrator_recipient_for_pty(state, pty_session) else {
        return;
    };
    // An idle edge is new evidence, not a repeat of the attempt that failed to
    // start: re-arm before chasing, or a single unclaimable composer silences
    // every later retry for the rest of the session.
    state.rearm_orchestrator_wake_budget(&recipient);
    chase_orchestrator_mail_wake(state, &recipient);
}

/// Re-offer an outstanding wake on the budget the recipient already has. Unlike
/// [`reevaluate_orchestrator_mail_wake`] this grants nothing, so it is what a
/// caller inside one delivery uses to chase its own leftovers.
fn chase_orchestrator_mail_wake(state: &AppState, recipient: &str) {
    let Some(needed_through) = state.orchestrator_wake_needed_through(recipient) else {
        return;
    };
    let _ = route_registered_orchestrator_mail(
        state,
        recipient,
        "tuic-orchestrator-mail-notice",
        needed_through,
    );
}

pub(super) fn apply_claimed_injection_outcome(
    state: &AppState,
    session_id: &str,
    text: &str,
    claim: InjectionClaim,
    outcome: InjectionOutcome,
    kind: ClaimedInjectionKind,
) -> InjectionOutcome {
    match &outcome {
        InjectionOutcome::Submitted => {
            commit_injection_claim(state, session_id, claim);
            if state
                .pending_initial_prompts
                .get(session_id)
                .is_some_and(|pending| pending.prompt == text)
            {
                // Delivery finally happened. If the watchdog already told the
                // parent it had not, close that loop rather than leaving the
                // parent holding a failure notice for work that is now running.
                let notified = state
                    .pending_initial_prompts
                    .remove(session_id)
                    .is_some_and(|(_, pending)| pending.notified);
                if let Some(session) = state.session_maps.sessions.get(session_id)
                    && let Some(receipt) = &mut session.lock().launch_receipt
                {
                    receipt.mark_brief_sent();
                }
                if notified {
                    notify_initial_prompt_delivered(state, session_id);
                }
            }
        }
        InjectionOutcome::NotStarted(error) => {
            tracing::debug!(session = %session_id, error, "agent command injection did not start");
            rollback_injection_claim(state, session_id, claim);
        }
        InjectionOutcome::Uncertain(error) => {
            tracing::warn!(session = %session_id, error, "agent command injection outcome uncertain; preserving busy state");
            match kind {
                ClaimedInjectionKind::OrchestratorNotice => {
                    mark_orchestrator_notice_uncertain(state, session_id, claim)
                }
                ClaimedInjectionKind::Message => mark_injection_uncertain(state, session_id, claim),
            }
        }
    }
    outcome
}

fn requeue_injection_front(
    state: &AppState,
    session_id: &str,
    injection: crate::state::PendingInjection,
) {
    state
        .pending_injections
        .entry(session_id.to_string())
        .or_default()
        .push_front(injection);
}

/// What actually became of a peer message handed to the terminal path.
///
/// The distinction is the whole point: `Queued` is NOT delivery. The composer was
/// busy, so the message only sits in `pending_injections` until the next BUSY→IDLE
/// transition — and a teardown before that flush drops the queue (see the tombstone
/// cleanup). Collapsing this into "the session still exists" is what let a caller
/// mark a never-typed message `TerminalDispatched`, which the waiter filter then
/// hides, stranding it in the inbox with nothing left to surface it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PtyDelivery {
    /// Written into the composer and submitted, or written ambiguously enough that
    /// a retry would risk duplicating it. Either way the terminal owns it.
    Typed,
    /// Parked in `pending_injections`; nothing reached the terminal yet.
    Queued,
    /// Not an agent, or the session is gone — the terminal path cannot take it.
    Unavailable,
}

/// Type a server-authored notice into a recipient's terminal, waking it. Injects
/// immediately when the recipient is an idle agent; otherwise queues it to flush
/// on the recipient's next BUSY→IDLE transition. No-op for non-agent sessions.
/// The caller has already buffered the authoritative copy in the inbox.
///
/// `framed` is a notice, never a peer payload: the payload-free `PEER_MAIL_WAKE`
/// or a child lifecycle summary. Peer `send` content stays in the inbox — see
/// `PendingInjection`.
pub(crate) fn deliver_notice_to_pty(
    state: &AppState,
    session_id: &str,
    framed: &str,
) -> PtyDelivery {
    // Never queue for a non-agent — shells and dead sessions have no wake path.
    if !session_is_agent(state, session_id) {
        return PtyDelivery::Unavailable;
    }
    if let Some(claim) = claim_idle_for_injection(state, session_id) {
        if matches!(
            run_claimed_injection(
                state,
                session_id,
                framed,
                claim,
                ClaimedInjectionKind::Message
            ),
            InjectionOutcome::NotStarted(_)
        ) {
            requeue_injection_front(
                state,
                session_id,
                crate::state::PendingInjection::notice(framed),
            );
            return PtyDelivery::Queued;
        }
        // Submitted, or Uncertain — an ambiguous write must not be retried, so the
        // terminal keeps ownership either way.
        PtyDelivery::Typed
    } else {
        // One parked mail wake covers the whole inbox: the recipient answers it
        // by reading every message. Pushing one per sender would type the same
        // pointer N times and, worse, keep the queue non-empty for N idle
        // windows — the state that blocks `submit`.
        let already_parked = framed == PEER_MAIL_WAKE
            && state
                .pending_injections
                .get(session_id)
                .is_some_and(|queue| queue.iter().any(|entry| entry.text() == PEER_MAIL_WAKE));
        if !already_parked {
            state
                .pending_injections
                .entry(session_id.to_string())
                .or_default()
                .push_back(crate::state::PendingInjection::notice(framed));
        }
        // CONC-A (story 101-20e3): the should_inject_now read above and this push are
        // not atomic vs a concurrent BUSY→IDLE flush. If the silence timer transitions
        // the session to idle and drains the (still-empty) queue in the window between
        // them, our message would sit queued until the NEXT idle cycle — exactly the
        // auto-wake this feature exists to deliver. Re-flush after enqueuing: if the
        // session went idle during the window, flush_pending_injections (self-guarded
        // by should_inject_now) delivers it ourselves. A double flush is harmless — it
        // drains under a get_mut write lock, so the racing flush that loses just finds
        // an empty queue.
        // Blocking on purpose: the emptiness check below IS the return value.
        flush_pending_injections_blocking(state, session_id);
        // That flush drains the whole queue under a write lock, so an empty queue
        // means everything — ours included — reached the composer. A non-empty
        // queue may still hold this message, and reporting Queued in the ambiguous
        // case is the safe direction: the worst outcome is that teardown later
        // hands a message the waiter can still see, instead of losing it.
        if state
            .pending_injections
            .get(session_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            PtyDelivery::Queued
        } else {
            PtyDelivery::Typed
        }
    }
}

/// Settle wake ownership from what the terminal path actually did.
///
/// `Queued` deliberately does nothing, and that is the fix: the message stays
/// `TerminalPending`, which is the truthful state — the terminal owns it and will
/// type it on the next idle transition, but nothing has been typed yet. Marking it
/// `TerminalDispatched` here (as every call site used to, because the old boolean
/// only meant "the session exists") claimed a delivery that had not happened.
pub(crate) fn settle_terminal_delivery(
    state: &AppState,
    tuic_session: &str,
    message_id: &str,
    outcome: PtyDelivery,
) {
    match outcome {
        PtyDelivery::Typed => state.mark_terminal_delivery_dispatched(tuic_session, message_id),
        PtyDelivery::Queued => {}
        PtyDelivery::Unavailable => state.release_terminal_delivery(tuic_session, message_id),
    }
}

/// Deliver only while the recipient still has a managed PTY and agent state.
/// Reports what the terminal path actually did, so the caller can keep wake
/// ownership only for a message that truly reached the composer. `Unavailable`
/// means teardown won the race and the authoritative inbox copy must stay
/// available to `agent wait`.
pub(crate) fn deliver_notice_to_managed_pty(
    state: &AppState,
    session_id: &str,
    framed: &str,
) -> PtyDelivery {
    let available = state.session_maps.sessions.contains_key(session_id)
        && state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.agent_type.is_some());
    if !available {
        return PtyDelivery::Unavailable;
    }
    let outcome = deliver_notice_to_pty(state, session_id, framed);
    // Teardown can still win between the check above and the write.
    if state.session_maps.sessions.contains_key(session_id) {
        outcome
    } else {
        PtyDelivery::Unavailable
    }
}

/// `flush_pending_injections_blocking` off the calling thread.
///
/// This is the entry point for every caller that runs on a tokio worker — the
/// session-state accumulator, the silence timer, desktop input bookkeeping.
/// The flush waits for agent acknowledgement as well as the Enter gaps. Give
/// each pending session its own worker so one silent agent cannot delay a
/// different session or ordered lifecycle notices on `INJECTION_QUEUE`.
/// Each idle edge with pending input may start another short-lived worker;
/// there is no global thread cap. A blocked claim returns immediately, while
/// the successful claim waits for the agent-specific acknowledgement bound.
///
/// Callers that must observe the result before returning — `deliver_notice_to_pty`
/// reads the queue to tell `Typed` from `Queued` — call the blocking form directly.
/// The OSC reader schedules this only after it publishes the current chunk;
/// confirmation needs that reader to remain free for the child's next chunk.
pub(crate) fn flush_pending_injections(
    state: &Arc<AppState>,
    session_id: &str,
) -> Option<std::thread::JoinHandle<()>> {
    if state
        .pending_injections
        .get(session_id)
        .is_none_or(|pending| pending.is_empty())
    {
        return None;
    }
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    match std::thread::Builder::new()
        .name("tuic-queued-submit".into())
        .spawn(move || flush_pending_injections_blocking(&state, &session_id))
    {
        Ok(worker) => Some(worker),
        Err(error) => {
            tracing::error!(%error, "queued injection worker could not start");
            None
        }
    }
}

/// Codex can swallow the Enter of a queued command (paste-burst suppression)
/// and keep the text in its composer. True when the tail of `text` is still on
/// the tracked screen, or a paste placeholder holds the composer. Only Codex is probed:
/// an Enter on its empty composer is a no-op, so a stale echo of already-submitted text costs nothing.
pub(crate) fn composer_retains_text(state: &AppState, session_id: &str, text: &str) -> bool {
    let is_codex = state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|session| session.agent_type.as_deref() == Some("codex"));
    if !is_codex {
        return false;
    }
    let squash = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let squashed = squash(text);
    let chars: Vec<char> = squashed.chars().collect();
    let tail: String = chars[chars.len().saturating_sub(COMPOSER_TAIL_CHARS)..]
        .iter()
        .collect();
    if tail.is_empty() {
        return false;
    }
    state.grid.vt_log_buffers.get(session_id).is_some_and(|vt| {
        let rows = vt.lock().screen_rows();
        squash(&composer_rows(&rows).join("\n")).contains(&tail)
            || composer_holds_paste_placeholder(&rows)
    })
}

/// A long paste collapses to a placeholder that never shows the text. It sits inline
/// in a wrapped composer, on a continuation row rather than the `›` row. With a live
/// `›` row found, only that composer block counts (a stale placeholder in history
/// must not draw a second Enter). A composer wrapped past the prompt window has no
/// findable `›` row, so the bottom rows are searched instead.
pub(super) fn composer_holds_paste_placeholder(rows: &[String]) -> bool {
    composer_rows(rows)
        .iter()
        .any(|row| row.contains(CODEX_PASTE_PLACEHOLDER))
}

/// The rows that can hold the composer text. With a live `›` row found, only its
/// block counts: Codex echoes accepted text as `› <text>` in the transcript above
/// an empty composer, and that echo is not retained text. A composer wrapped past
/// the prompt window has no findable `›` row, so the bottom rows are used instead.
fn composer_rows(rows: &[String]) -> Vec<&str> {
    if let Some(prompt) = find_codex_prompt_row(rows) {
        return rows[prompt..]
            .iter()
            .enumerate()
            .take_while(|(offset, row)| *offset == 0 || !row.trim().is_empty())
            .map(|(_, row)| row.as_str())
            .collect();
    }
    let mut bottom: Vec<&str> = rows
        .iter()
        .rev()
        .filter(|row| !row.trim().is_empty())
        .take(COMPOSER_BOTTOM_ROWS)
        .map(String::as_str)
        .collect();
    bottom.reverse();
    bottom
}

/// What Codex shows in its composer in place of a long pasted text.
const CODEX_PASTE_PLACEHOLDER: &str = "[Pasted Content";

/// Non-empty bottom rows searched for the paste placeholder: a wrapped composer plus its footer.
const COMPOSER_BOTTOM_ROWS: usize = 8;

/// Number of trailing characters of a queued command searched for on screen.
const COMPOSER_TAIL_CHARS: usize = 32;

/// One bare Enter for a composer that still holds the queued text after an
/// unconfirmed submission. Never repeated: the caller reports uncertainty when
/// this returns false.
pub(super) fn retry_enter_for_retained_composer(
    state: &AppState,
    session_id: &str,
    text: &str,
    initial_screen: &'static str,
) -> bool {
    if !composer_retains_text(state, session_id, text) {
        return false;
    }
    let Some(writer) = state.pty_writer(session_id) else {
        return false;
    };
    let offset = state
        .session_maps
        .output_buffers
        .get(session_id)
        .map(|buffer| buffer.lock().total_written)
        .unwrap_or(0);
    {
        let mut writer = writer.lock();
        state.retire_turn_interrupt(session_id);
        if write_all_with_progress(writer.as_mut(), b"\r", 0).is_err() || writer.flush().is_err() {
            return false;
        }
    }
    wait_for_queued_submission(state, session_id, offset, initial_screen)
}

/// A PTY write only proves that Enter reached the master. Wait for output from
/// the child and a working-screen or agent-hook signal before settling a queued
/// command. A silent or unrecognised composer remains uncertain, so its text
/// cannot be automatically replayed into a possibly active turn.
fn wait_for_queued_submission(
    state: &AppState,
    session_id: &str,
    offset: u64,
    initial_screen: &'static str,
) -> bool {
    if !state.grid.vt_log_buffers.contains_key(session_id)
        || !state.session_maps.output_buffers.contains_key(session_id)
    {
        return false;
    }
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let profile = agent_submit_profile(agent_type.as_deref());
    let confirmation_window = if matches!(agent_type.as_deref(), Some("codex" | "claude")) {
        DELAYED_AGENT_QUEUED_SUBMISSION_CONFIRMATION
    } else {
        std::time::Duration::from_secs(1)
    };
    let deadline = std::time::Instant::now() + confirmation_window;
    loop {
        let output_advanced = state
            .session_maps
            .output_buffers
            .get(session_id)
            .is_some_and(|buffer| buffer.lock().total_written > offset);
        if output_advanced {
            let hook_busy = state
                .session_maps
                .silence_states
                .get(session_id)
                .is_some_and(|silence| {
                    let silence = silence.lock();
                    silence.busy_source_is("hook-busy")
                        || silence.last_hook_busy_offset.is_some_and(|at| at >= offset)
                });
            let screen_state = agent_submission_ack_kind(state, session_id);
            let fresh_working_transition = state
                .session_maps
                .silence_states
                .get(session_id)
                .is_some_and(|silence| {
                    let silence = silence.lock();
                    silence.last_ready_screen_offset > offset
                        && silence.last_working_screen_offset > silence.last_ready_screen_offset
                });
            // A Working screen recorded after the Enter boundary proves the turn
            // started even when a later screen (question overlay, ready) already
            // replaced it before this observer ran.
            let working_since_enter = state
                .session_maps
                .silence_states
                .get(session_id)
                .is_some_and(|silence| silence.lock().last_working_screen_offset > offset);
            let screen_confirms = match profile.confirmation {
                SubmitConfirmation::WorkingScreen if initial_screen != "working_screen" => {
                    screen_state == "working_screen" || working_since_enter
                }
                SubmitConfirmation::WorkingScreen => {
                    screen_state == "working_screen" && fresh_working_transition
                }
                SubmitConfirmation::PromptGone => {
                    initial_screen == "ready_screen" && screen_state == "terminal_output"
                }
                SubmitConfirmation::HookOnly | SubmitConfirmation::LegacyWrite => false,
            };
            if hook_busy || screen_confirms {
                return true;
            }
        }
        if !state.session_maps.sessions.contains_key(session_id) {
            return false;
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return false;
        }
        std::thread::sleep((deadline - now).min(std::time::Duration::from_millis(25)));
    }
}

/// Drain and inject any messages queued for a session that can receive them now.
/// Self-guarded by `should_inject_now`: skips (leaves queued) unless the session
/// is an idle agent not blocked on a confident question, so a peer message never
/// answers a user-facing approval prompt and never corrupts a busy TUI. Called
/// from the BUSY→IDLE transition, the post-enqueue race re-check, and the
/// unblock path when a confident question clears while the agent is idle.
pub(crate) fn flush_pending_injections_blocking(state: &AppState, session_id: &str) {
    if state
        .pending_injections
        .get(session_id)
        .is_none_or(|pending| pending.is_empty())
    {
        return;
    }
    let queued_before = queued_command_count(state, session_id);
    let snapshot = state.session_state_with_shell(session_id);
    let agent_state = snapshot
        .as_ref()
        .and_then(|session| session.agent_state.as_deref())
        .unwrap_or("unknown");
    let shell_state = snapshot
        .as_ref()
        .and_then(|session| session.shell_state.as_deref())
        .unwrap_or("unknown");
    let claim = match claim_idle_for_injection(state, session_id) {
        Some(claim) => claim,
        None => {
            let defer_reason = if !session_is_agent(state, session_id) {
                "not_agent"
            } else if shell_state != "idle" {
                "shell_not_idle"
            } else if !idle_is_confirmed(state, session_id) {
                "idle_unconfirmed"
            } else if blocked_on_confident_question(state, session_id) {
                "confident_question"
            } else if has_partial_user_input(state, session_id) {
                "partial_composer"
            } else {
                "claim_lost"
            };
            tracing::info!(
                session_id,
                agent_state,
                shell_state,
                defer_reason,
                queued_before,
                queued_after = queued_before,
                typed = "no",
                submitted = false,
                enter_separate = "not_sent",
                "queue delivery attempt deferred"
            );
            return;
        }
    };
    let pending = state
        .pending_injections
        .get_mut(session_id)
        .and_then(|mut queue| queue.pop_front());
    let Some(injection) = pending else {
        tracing::info!(
            session_id,
            agent_state,
            shell_state,
            queued_before,
            queued_after = queued_command_count(state, session_id),
            typed = "no",
            submitted = false,
            enter_separate = "not_sent",
            "queue delivery attempt lost to another flush"
        );
        return;
    };
    let (outcome, enter_retry) = submit_and_confirm(state, session_id, injection.text());
    let outcome = apply_claimed_injection_outcome(
        state,
        session_id,
        injection.text(),
        claim,
        outcome,
        ClaimedInjectionKind::Message,
    );
    let (typed, submitted, enter_separate) = match outcome {
        InjectionOutcome::Submitted => ("yes", true, "sent"),
        InjectionOutcome::NotStarted(_) => ("no", false, "not_sent"),
        InjectionOutcome::Uncertain(_) => ("uncertain", false, "uncertain"),
    };
    if matches!(outcome, InjectionOutcome::NotStarted(_)) {
        requeue_injection_front(state, session_id, injection);
    }
    if let InjectionOutcome::Uncertain(ref reason) = outcome {
        let title = "Agent input was not confirmed";
        let message = format!(
            "Do not retype this message. Check the agent transcript and composer; if the text remains in the composer, press Enter once. {reason}"
        );
        emit_session_toast(state, session_id, title, message, "error");
    }
    tracing::info!(
        session_id,
        agent_state,
        shell_state,
        queued_before,
        queued_after = queued_command_count(state, session_id),
        typed,
        submitted,
        enter_separate,
        enter_retry,
        "queue delivery attempt"
    );
}

/// Everything still parked for a session, of any kind.
///
/// Counting only `UserCommand` here is what made a stuck queue undiagnosable: a
/// single server entry blocked `submit` with `queued_commands_pending` while the
/// Compose badge read 0 and the listing was empty, so there was nothing to see
/// and nothing to delete. Every parked entry is now counted, listed and
/// removable; `kind` is what tells them apart.
pub(crate) fn queued_command_count(state: &AppState, session_id: &str) -> usize {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| queue.len())
        .unwrap_or(0)
}

/// One parked entry, as the Compose panel lists it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct QueuedCommand {
    pub id: u64,
    pub text: String,
    /// `user_command`, `notice` or `initial_prompt` — see `PendingInjection`.
    pub kind: &'static str,
}

/// Everything still parked, in delivery order.
pub(crate) fn list_queued_commands(state: &AppState, session_id: &str) -> Vec<QueuedCommand> {
    state
        .pending_injections
        .get(session_id)
        .map(|queue| {
            queue
                .iter()
                .map(|entry| QueuedCommand {
                    id: entry.id(),
                    text: entry.text().to_string(),
                    kind: entry.kind(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Drop a single parked entry by id. Returns false when the id is unknown —
/// the entry may have been typed already, which is not an error for the caller.
pub(crate) fn remove_queued_command(state: &AppState, session_id: &str, id: u64) -> bool {
    state
        .pending_injections
        .get_mut(session_id)
        .map(|mut queue| {
            let before = queue.len();
            queue.retain(|entry| entry.id() != id);
            before != queue.len()
        })
        .unwrap_or(false)
}

/// What the idle gate did with a user-composed command.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct EnqueuedCommand {
    /// Accepted now or recognized as an earlier keyed acceptance; not a PTY receipt.
    pub accepted: bool,
    /// The agent was already idle, so the text was typed and submitted at once.
    pub typed: bool,
    /// Commands still waiting, this one included when `typed` is false.
    pub queued: usize,
}

/// Route a user-composed command through the same idle gate peer messages use:
/// typed immediately when the agent is idle, otherwise parked until the next
/// BUSY→IDLE transition. This is the whole point of the Compose panel's enqueue
/// action — the user wants the text delivered *without* steering a running turn.
///
/// The command is always appended before the flush, never handed straight to
/// `deliver_notice_to_pty`: injecting ahead of any accepted peer message or
/// Compose command would reorder delivery. `flush_pending_injections_blocking` pops one
/// typed entry and leaves the session BUSY, so the shared queue drains one item
/// per idle transition and stays FIFO across both producers.
///
/// Agent sessions only (see `session_is_agent`). Keyed retries are recognized
/// atomically with append; keys remain recent after the FIFO drains.
pub(crate) fn enqueue_user_command(
    state: &AppState,
    session_id: &str,
    text: &str,
    idempotency_key: Option<&str>,
) -> Result<EnqueuedCommand, String> {
    if let Some(key) = idempotency_key
        && (key.is_empty() || key.len() > 128)
    {
        return Err("Invalid idempotency key".into());
    }
    if text.trim().is_empty() {
        return Err("Command text is empty".to_string());
    }
    if !state.session_maps.sessions.contains_key(session_id) {
        return Err("Session not found".to_string());
    }
    if !session_is_agent(state, session_id) {
        return Err("Session is not running an agent".to_string());
    }
    let queued = if let Some(key) = idempotency_key {
        // Hold this per-session entry only through append, never through the
        // blocking flush. Concurrent retries cannot both reserve the same key.
        let mut keys = state
            .recent_queue_keys
            .entry(session_id.to_string())
            .or_default();
        if keys.iter().any(|recent| recent == key) {
            return Ok(EnqueuedCommand {
                accepted: true,
                typed: false,
                queued: queued_command_count(state, session_id),
            });
        }
        state
            .pending_injections
            .entry(session_id.to_string())
            .or_default()
            .push_back(crate::state::PendingInjection::user_command(text));
        // Bounded in-memory retry window: 128 completions per live PTY.
        if keys.len() == 128 {
            keys.pop_front();
        }
        keys.push_back(key.to_string());
        drop(keys);
        flush_pending_injections_blocking(state, session_id);
        queued_command_count(state, session_id)
    } else {
        let (_, _, queued) = append_and_flush(
            state,
            session_id,
            crate::state::PendingInjection::user_command(text),
        );
        queued
    };
    // An empty queue after the flush means our command was the only one waiting
    // and reached the composer; any remaining entry means it is still parked.
    Ok(EnqueuedCommand {
        accepted: true,
        typed: queued == 0,
        queued,
    })
}

/// Append one entry and run the idle gate over the queue.
///
/// Returns the entry's id, whether that entry is the one the flush typed, and
/// how many entries remain. Unkeyed enqueue uses this path; keyed enqueue
/// reserves acceptance with append before invoking the same flush.
/// `typed` is read back per entry rather than from an emptied queue:
/// a command behind a peer notice is still parked even though the flush
/// delivered something.
fn append_and_flush(
    state: &AppState,
    session_id: &str,
    injection: crate::state::PendingInjection,
) -> (u64, bool, usize) {
    let id = injection.id();
    state
        .pending_injections
        .entry(session_id.to_string())
        .or_default()
        .push_back(injection);
    // Blocking on purpose: the counts below are read from the post-flush queue.
    flush_pending_injections_blocking(state, session_id);
    let still_parked = state
        .pending_injections
        .get(session_id)
        .is_some_and(|queue| queue.iter().any(|entry| entry.id() == id));
    (id, !still_parked, queued_command_count(state, session_id))
}

/// Type one hands-free turn into an agent's composer now — busy or idle.
///
/// This is the *only* way hands-free speech reaches a model, and it is not the
/// Compose queue: that queue is for "one message, let the agent work, then the
/// next". Speech behaves like a line the user types by hand into a working
/// agent, which the agent queues or takes mid-turn itself (Boss's rule;
/// parking it until idle measured a median 103 s, max 594 s).
///
/// Two holds remain, and they are the reason this is not a raw write: a
/// confident question (speech must never answer a permission dialog) and a
/// draft in the composer. A held turn stays with the caller. The write itself
/// is the framed path every injection uses (Ctrl-U, text, a separate Enter).
#[cfg(feature = "dictation")]
pub(crate) fn write_voice_turn(
    state: &AppState,
    session_id: &str,
    text: &str,
) -> Result<VoiceWrite, String> {
    if text.trim().is_empty() {
        return Err("Command text is empty".to_string());
    }
    if !session_accepts_voice(state, session_id) {
        return Err(if state.session_maps.sessions.contains_key(session_id) {
            "Session is not running an agent".to_string()
        } else {
            "Session not found".to_string()
        });
    }
    if blocked_on_confident_question(state, session_id) {
        return Ok(VoiceWrite::Held(VoiceHold::Question));
    }
    if has_partial_user_input(state, session_id) {
        return Ok(VoiceWrite::Held(VoiceHold::Draft));
    }
    let Some(claim) = claim_composer_for_voice(state, session_id) else {
        return Ok(VoiceWrite::Held(
            if has_partial_user_input(state, session_id) {
                VoiceHold::Draft
            } else {
                VoiceHold::InFlight
            },
        ));
    };
    Ok(
        match run_claimed_injection(
            state,
            session_id,
            text,
            claim,
            ClaimedInjectionKind::Message,
        ) {
            InjectionOutcome::Submitted | InjectionOutcome::Uncertain(_) => VoiceWrite::Written,
            InjectionOutcome::NotStarted(_) => VoiceWrite::Held(VoiceHold::WriteNotStarted),
        },
    )
}

/// Whether a session can take hands-free speech at all.
///
/// The same two conditions `write_voice_turn` enforces, asked *before* arming
/// so an unsupported target is refused where the user can see it rather than
/// after the first utterance. Deliberately not a "can we reach it somehow"
/// check: an ACP target has no PTY composer, and there is no fallback for it.
#[cfg(feature = "dictation")]
pub(crate) fn session_accepts_voice(state: &AppState, session_id: &str) -> bool {
    state.session_maps.sessions.contains_key(session_id) && session_is_agent(state, session_id)
}

/// Drop everything still waiting for this session. Returns the count removed.
///
/// This is the drain a stuck queue needs: leaving server entries behind meant
/// "Clear" emptied the visible list while the composer stayed blocked on what
/// was left. Nothing here is load-bearing — a dropped mail wake costs at most
/// one `agent action=inbox` (the mail itself never left the inbox), and a
/// dropped initial prompt is still on record in `pending_initial_prompts`.
pub(crate) fn clear_queued_commands(state: &AppState, session_id: &str) -> usize {
    state
        .pending_injections
        .get_mut(session_id)
        .map(|mut queue| {
            let before = queue.len();
            queue.clear();
            before
        })
        .unwrap_or(0)
}

#[cfg(feature = "desktop")]
pub(super) async fn write_pty_parts_off_thread(
    state: Arc<AppState>,
    session_id: String,
    parts: Vec<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || write_pty_parts_blocking(&state, &session_id, &parts))
        .await
        .map_err(|e| format!("Task join error: {e}"))?
}

#[cfg(feature = "desktop")]
fn write_pty_parts_blocking(
    state: &Arc<AppState>,
    session_id: &str,
    parts: &[String],
) -> Result<(), String> {
    // Keystroke delivery to the PTY: run on the high QoS band so the write (and
    // thus the echo round-trip) isn't starved by a saturating build. Scoped, not
    // a bare raise: this thread comes from the shared blocking pool and goes back
    // into it, so an unrestored bump would promote the pool the terminal is
    // trying to out-schedule.
    let _qos = interactive_io_boost();
    // Restore cursor if hidden — Ink-based agents send DECTCEM hide for
    // spinners but may not send CNORM when returning to the prompt.
    // Best-effort try_lock: this is cosmetic (touches the local grid, not the PTY)
    // and MUST NOT block input delivery. Under an output flood the ticker
    // (serialize_dirty_rows) and reader thrash this same vt lock; a blocking lock
    // here would starve input. If contended, skip — the next frame restores the
    // cursor anyway.
    if let Some(vt) = state.grid.vt_log_buffers.get(session_id)
        && let Some(mut vt) = vt.try_lock()
        && !vt.is_cursor_visible()
    {
        vt.process(b"\x1b[?25h");
    }

    let data_len: usize = parts.iter().map(String::len).sum();
    tracing::trace!(session_id = %session_id, data_len, part_count = parts.len(), "write_pty");
    for data in parts {
        if data.contains("\x1b[?1049")
            || data.contains("\x1b[?1047")
            || data.contains("\x1b[?47l")
            || data.contains("\x1b[?25h")
        {
            tracing::error!(source = "terminal", session_id = %session_id,
                "write_pty received DEC private mode sequences! data({} bytes)={:?}",
                data.len(), data.as_bytes().iter().take(200).collect::<Vec<_>>());
        }
    }

    let byte_parts: Vec<&[u8]> = parts.iter().map(|part| part.as_bytes()).collect();
    let t0 = std::time::Instant::now();
    state.write_pty_parts(session_id, &byte_parts)?;
    let total_ms = t0.elapsed().as_millis();
    if total_ms > 100 {
        tracing::warn!(session_id = %session_id, total_ms = %total_ms,
            data_len, part_count = parts.len(), "write_pty SLOW — lock or write blocked");
    }

    for data in parts {
        if crate::pty_capture::is_enabled() {
            let capture_geometry = state.grid.vt_log_buffers.get(session_id).map(|vt| {
                let vt = vt.lock();
                (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
            });
            crate::pty_capture::record_input_with_geometry(
                session_id,
                data.as_bytes(),
                capture_geometry,
            );
        }
        apply_desktop_input_bookkeeping(state, session_id, data);
    }

    Ok(())
}

#[cfg(feature = "desktop")]
fn apply_desktop_input_bookkeeping(state: &Arc<AppState>, session_id: &str, data: &str) {
    // Stamp last-input time so the grid ticker can throttle frame sends while
    // the user types under CPU saturation (keeps the WebView thread free for
    // keystroke dispatch + echo).
    stamp_input_ms(state, session_id);
    crate::state::resolve_choice_prompt_input(state, session_id, data);

    // Feed input through the line buffer to reconstruct user-typed lines.
    // Release both the inner mutex and DashMap entry guard before callbacks
    // below. In particular, flush_pending_injections -> should_inject_now
    // reads input_buffers again; retaining input_entry there self-deadlocks
    // this shard and can park the entire IPC Tokio runtime under load.
    let (actions, buffer_empty, buffer_is_slash) = {
        let input_entry = state
            .session_maps
            .input_buffers
            .entry(session_id.to_string())
            .or_insert_with(|| parking_lot::Mutex::new(InputLineBuffer::new()));
        let mut buf = input_entry.lock();
        let actions = buf.feed(data);
        // Two bits, not the line: `content()` here collected the whole typed
        // line into a fresh String on every keystroke, to be dropped below.
        (actions, buf.is_empty(), buf.starts_with('/'))
    };
    let mut line_submitted = false;
    for action in actions {
        match action {
            InputAction::Line(content) => {
                line_submitted = true;
                // Keystroke-reconstructed: no grid context, so no prompt row
                // (line = -1). The OSC 7770 busy path supplies an absolute
                // scrollbar marker when available.
                record_submitted_line(state, session_id, content, -1);
            }
            InputAction::Interrupt => {
                line_submitted = true;
                if let Some(ss) = state.session_maps.silence_states.get(session_id) {
                    ss.lock().note_interrupt_requested();
                }
            }
        }
    }
    // Codex advertises Escape as its normal interrupt key. A bare Escape is
    // only intent evidence; it never flips idle until the agent redraws an
    // interrupted/ready prompt. CSI-prefixed navigation keys are excluded.
    if data == "\x1b"
        && let Some(ss) = state.session_maps.silence_states.get(session_id)
    {
        ss.lock().note_interrupt_requested();
    }

    // On any line submit (Enter or Ctrl+C) reset the tool-error dedup
    // memory: the user is explicitly engaging again, so a recurrence of
    // the same failure in a later turn must be allowed to notify.
    // Mirrors `OutputParser`'s reset of `last_api_error_match` on UserInput.
    if line_submitted && let Some(ss) = state.session_maps.silence_states.get(session_id) {
        let mut sl = ss.lock();
        sl.reset_tool_error_memory();
        sl.reset_suggest_memory();
    }

    // Track slash command mode: true when the input buffer starts with /
    // Fallback: when ESC is sent before "/" (TerminalKeybar's handleSlash),
    // the InputLineBuffer consumes "/" as an unknown escape-sequence suffix
    // and never inserts it. Detect bare "/" writes that the buffer missed.
    let in_slash = if line_submitted {
        false
    } else {
        buffer_is_slash || (buffer_empty && data == "/")
    };
    // Look up before inserting: `entry` needs an owned key, so the allocation is
    // paid only when the map entry does not already exist.
    match state.session_maps.slash_mode.get(session_id) {
        Some(flag) => flag.store(in_slash, std::sync::atomic::Ordering::Relaxed),
        None => {
            state
                .session_maps
                .slash_mode
                .entry(session_id.to_string())
                .or_insert_with(|| std::sync::atomic::AtomicBool::new(false))
                .store(in_slash, std::sync::atomic::Ordering::Relaxed);
        }
    }

    if buffer_empty {
        flush_pending_injections(state, session_id);
    }
}
