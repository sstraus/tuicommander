//! One ego turn, start to finish, with nobody watching.
//!
//! The AI Chat panel drives a conversation a person is sitting in front of.
//! This drives a single turn nobody is looking at: a Smart Prompt in `api`
//! mode asks ego to transform some text and wants the answer back, once. There
//! is no transcript, no stream to render and no second turn.
//!
//! Three rules follow from "nobody is watching", and they are the whole design:
//!
//! - **No host tools.** [`AcpClientManager::new_session`] gives every session
//!   TUICommander's own MCP server, which is how ego reaches terminals and
//!   repositories. Handing that to a turn with no one at the keyboard is an
//!   authority nobody granted, so an unattended session is opened with no MCP
//!   server at all.
//! - **Every question is refused, immediately.** Ego still has tools of its
//!   own, and it may ask permission to use one. A seat nobody takes is a turn
//!   that never ends, so each request is settled the instant it arrives with
//!   the answer that grants nothing — the same `Cancelled`/`Cancel` pair the
//!   connection already uses when there is no one left to tell. How many were
//!   refused is reported, because "the model wanted a tool this mode cannot
//!   give it" is the difference between a diagnosis and an empty box.
//! - **The connection is this call's.** It is launched here and shut down here,
//!   rather than borrowed from whatever the panel happens to have open. A Smart
//!   Prompt must run with the panel closed, and sharing would couple a
//!   one-shot to a conversation's lifetime for the sake of one process launch.
//!
//! The fold from events to an answer is [`TurnCollector`], which is pure and
//! carries the tests. Everything around it is I/O.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1;
use serde::Serialize;

use super::{
    AcpClientError, AcpClientEvent, AcpConnectRequest, AcpConnectionId, AcpConnectionState,
    AcpHostRequestId, AcpSessionAuthority,
};
use crate::acp_commands::ego_config;
use crate::state::AppState;

/// How long the turn itself may take, once ego is up: `session/new`, the
/// prompt, and everything ego then says.
///
/// Server-side and not a parameter: no caller needs a different value, and a
/// timeout an HTTP body could choose is a way to pin an ego process for as long
/// as the sender likes.
///
/// It is 240s rather than the 300s it used to be because it now has to share
/// the router's budget with the launch. `/acp/one-shot` is wrapped by
/// `mcp_http::REQUEST_TIMEOUT` (301s), and this deadline used to bound only
/// [`collect`] — the ego launch, `initialize`, `session/new` and the prompt all
/// sat outside it, so a turn that used its full 300s of thinking overshot the
/// router and came back as a bare 408 instead of the sentence below.
/// `INITIALIZE_TIMEOUT` (60s) plus this is 300s, the same deliberate one-second
/// margin `CONFIRM_TIMEOUT` keeps, and
/// `request_timeout_beats_every_in_handler_deadline` pins it.
pub(crate) const TURN_TIMEOUT: Duration = Duration::from_secs(240);

/// What one unattended turn produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EgoTurn {
    /// Everything ego said, in order, with the surrounding blank space removed.
    pub text: String,
    /// How the turn ended, in ego's own vocabulary.
    pub stop_reason: v1::StopReason,
    /// How many questions were refused because nobody was there to answer.
    ///
    /// Non-zero with empty `text` is the diagnosis for the one failure this
    /// mode has that a person would not expect: the model reached for a tool
    /// and an unattended turn cannot grant one.
    pub declined: usize,
    /// Internal provenance; the one-shot wire response remains unchanged.
    #[serde(skip)]
    pub session_id: Option<String>,
}

/// A request that must be settled without granting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Permission(AcpHostRequestId),
    Elicitation(AcpHostRequestId),
}

/// What the driver must do after folding one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Keep reading.
    Continue,
    /// Settle this request, then keep reading.
    Refuse(Refusal),
    /// The turn is over; the collector holds the answer.
    Done,
}

/// The fold from a connection's events to one turn's answer.
///
/// Pure on purpose: everything that decides what the answer *is* lives here and
/// is tested against event sequences, while the driver around it only moves
/// bytes. The alternative — deciding inside the read loop — is testable only
/// against a live ego.
#[derive(Debug, Default)]
pub(crate) struct TurnCollector {
    text: String,
    declined: usize,
    stop_reason: Option<v1::StopReason>,
    failure: Option<String>,
}

impl TurnCollector {
    /// Fold one event and say what the driver owes the agent.
    pub(crate) fn observe(&mut self, event: &AcpClientEvent) -> Step {
        match event {
            AcpClientEvent::SessionUpdate { update } => {
                // Only what ego *said*. A thought chunk is reasoning on the way
                // to the answer, and a tool call is work, not an answer; either
                // one pasted into a terminal by `outputTarget` would be the
                // model thinking out loud in a person's shell.
                if let v1::SessionUpdate::AgentMessageChunk(chunk) = update.as_ref()
                    && let v1::ContentBlock::Text(text) = &chunk.content
                {
                    self.text.push_str(&text.text);
                }
                Step::Continue
            }
            AcpClientEvent::PermissionRequested { request_id, .. } => {
                self.declined += 1;
                Step::Refuse(Refusal::Permission(*request_id))
            }
            AcpClientEvent::ElicitationRequested { request_id, .. } => {
                self.declined += 1;
                Step::Refuse(Refusal::Elicitation(*request_id))
            }
            AcpClientEvent::TurnSettled { stop_reason, .. } => {
                self.stop_reason = Some(*stop_reason);
                Step::Done
            }
            AcpClientEvent::TurnFailed { message, .. } => {
                self.failure = Some(message.clone());
                Step::Done
            }
            // The process is gone. Reading on would wait for a turn that can no
            // longer end, so stop and let the driver report what it has.
            AcpClientEvent::ConnectionState {
                state:
                    AcpConnectionState::Closed | AcpConnectionState::Failed | AcpConnectionState::Killed,
            } => Step::Done,
            _ => Step::Continue,
        }
    }

    /// The finished turn, if one finished.
    ///
    /// `None` means the stream ended without ego ever settling the turn — a
    /// different thing from a turn that ended with nothing to say, and the
    /// driver reports it as such rather than as an empty answer.
    pub(crate) fn finish(self) -> Option<EgoTurn> {
        Some(EgoTurn {
            text: self.text.trim().to_string(),
            stop_reason: self.stop_reason?,
            declined: self.declined,
            session_id: None,
        })
    }

    pub(crate) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
}

/// The JSON object inside one ego answer.
///
/// A model fences JSON, or introduces it, or thanks you for asking, and saying
/// "only JSON" in the prompt does not make that stop. First `{` to last `}` is
/// the whole rule. It is deliberately not a parser: an answer with no object in
/// it comes back unchanged, so the failure surfaces as a parse error carrying
/// what ego actually said instead of as an empty result.
pub(crate) fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => &text[start..=end],
        _ => text.trim(),
    }
}

/// What an unattended turn said, or why it said nothing usable.
///
/// Three failures a person has to be able to tell apart, and none of them may
/// reach a panel as an empty list: ego reached for a tool this mode cannot
/// grant, ego finished with nothing to say, and — upstream of this — ego never
/// ran at all. The first is the one that looks like a bug and is not.
pub(crate) fn answer_text(turn: EgoTurn) -> Result<String, String> {
    if !turn.text.is_empty() {
        return Ok(turn.text);
    }
    if turn.declined > 0 {
        return Err(format!(
            "ego asked to use {} tool{} that an unattended turn cannot grant, and produced no answer",
            turn.declined,
            if turn.declined == 1 { "" } else { "s" }
        ));
    }
    Err("ego finished the turn without saying anything".to_string())
}

/// How much of an unusable answer is quoted back in the error.
const MAX_QUOTED: usize = 300;

/// Quote an answer back without pasting a whole transcript into a toast.
fn quoted(text: &str) -> String {
    match text.char_indices().nth(MAX_QUOTED) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// Ask ego one question and get back text worth parsing, or a reason.
///
/// The seam every unattended feature sits on, and the reason it is one function
/// rather than three copies: what each of them does with the answer differs, but
/// how they must *fail* does not. A turn can fail because ego is not configured,
/// because ego would not start, because ego reached for a tool this mode cannot
/// grant, or because ego said nothing — and each of those has to arrive at a
/// panel as its own sentence. Written out three times, two of them would
/// eventually become an empty list.
///
/// `system` rides the prompt rather than a system slot: `session/prompt` has no
/// separate one, and the alternative — writing ego's own config — is forbidden
/// by the plan (§6.2, decision 2).
pub(crate) async fn ask(
    state: &Arc<AppState>,
    root: PathBuf,
    system: &str,
    content: String,
) -> Result<String, String> {
    let turn = run_prompt(state, root, format!("{system}\n\n{content}"))
        .await
        .map_err(|err| format!("ego could not run this turn: {}", err.message))?;
    answer_text(turn)
}

/// Report an answer that would not parse, with enough of it to act on.
///
/// Shared so that "ego answered, but not with what this needs" reads the same
/// wherever it happens, and so that quoting a runaway answer cannot paste a
/// transcript into a toast.
pub(crate) fn unparseable(what: &str, err: impl std::fmt::Display, answer: &str) -> String {
    format!(
        "ego did not answer with the {what} this needs ({err}). It said: {}",
        quoted(answer)
    )
}

/// Reserved cancellation handles close the cancel-before-start race without
/// changing the lifetime of existing one-shot callers.
#[derive(Default)]
pub(crate) struct OneShotRequests {
    pending: parking_lot::Mutex<std::collections::HashMap<String, PreparedTurn>>,
}

struct PreparedTurn {
    cancel: tokio::sync::watch::Sender<bool>,
    receiver: Option<tokio::sync::watch::Receiver<bool>>,
}

impl OneShotRequests {
    fn reserve(&self) -> String {
        let id = uuid::Uuid::now_v7().to_string();
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        self.pending.lock().insert(
            id.clone(),
            PreparedTurn {
                cancel,
                receiver: Some(receiver),
            },
        );
        id
    }

    fn claim(&self, id: &str) -> Result<tokio::sync::watch::Receiver<bool>, AcpClientError> {
        self.pending
            .lock()
            .get_mut(id)
            .and_then(|turn| turn.receiver.take())
            .ok_or_else(|| {
                AcpClientError::invalid_input(
                    "One-shot request cancelled, expired or already started",
                )
            })
    }

    fn cancel(&self, id: &str) -> bool {
        if let Some(turn) = self.pending.lock().remove(id) {
            turn.cancel.send_replace(true);
            true
        } else {
            false
        }
    }
}

/// Reserve before launching, so Cancel also works while the frontend is
/// waiting for the start call to be accepted. An abandoned reservation expires
/// at the same outer budget as a one-shot (60s initialize + 240s turn).
pub(crate) fn prepare(state: &Arc<AppState>) -> String {
    let id = state.one_shot_requests.reserve();
    let weak = Arc::downgrade(state);
    let expiry_id = id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(301)).await;
        if let Some(state) = weak.upgrade() {
            state.one_shot_requests.cancel(&expiry_id);
        }
    });
    id
}

pub(crate) fn cancel(state: &AppState, request_id: &str) -> bool {
    state.one_shot_requests.cancel(request_id)
}

pub(crate) async fn run_prepared(
    state: &Arc<AppState>,
    root: PathBuf,
    prompt: String,
    request_id: String,
) -> Result<EgoTurn, AcpClientError> {
    let receiver = state.one_shot_requests.claim(&request_id)?;
    let state = Arc::clone(state);
    detached(async move {
        let outcome = run_turn_cancellable(&state, root, prompt, receiver).await;
        state.one_shot_requests.pending.lock().remove(&request_id);
        outcome
    })
    .await
}

/// Run one prompt through a fresh ego and give back what it said.
///
/// Every exit shuts the connection down, including the failing ones: an ego
/// left running for a turn nobody is waiting on is a process that will only be
/// noticed when there are a dozen of them.
pub(crate) async fn run_prompt(
    state: &Arc<AppState>,
    root: PathBuf,
    prompt: String,
) -> Result<EgoTurn, AcpClientError> {
    let state = Arc::clone(state);
    detached(async move { run_turn(&state, root, prompt).await }).await
}

/// Run one turn on its own task, so a caller that goes away cannot cancel it.
///
/// `/acp/one-shot` sits under the router's `REQUEST_TIMEOUT`, and
/// `tower_http`'s timeout DROPS the handler future when it fires; a browser
/// that disconnects mid-request does the same. Everything after the drop point
/// is then simply skipped — including the `disconnect` that stops the ego
/// process — and nothing else would ever stop it: the supervisor holding the
/// child is an independent task, and `forget_stale_settled` prunes settled
/// connections only, so the connection stays registered with a live process
/// until the app exits.
///
/// Awaiting a `JoinHandle` makes the caller's interest and the work's lifetime
/// two different things: dropping the handle abandons the answer, not the turn.
async fn detached<T: Send + 'static>(
    body: impl Future<Output = Result<T, AcpClientError>> + Send + 'static,
) -> Result<T, AcpClientError> {
    match tokio::spawn(body).await {
        Ok(outcome) => outcome,
        // Re-raised rather than converted: with the body inline a panic unwound
        // through the caller, and there is no `AcpClientError` that honestly
        // means "we have a bug" — dressing one up as a protocol failure would
        // send the next reader to ego. Nothing aborts this task, so
        // `into_panic` cannot be the cancelled case.
        Err(join) => std::panic::resume_unwind(join.into_panic()),
    }
}

/// Launch, ask, shut down. Runs to completion whatever the caller does.
async fn run_turn(
    state: &AppState,
    root: PathBuf,
    prompt: String,
) -> Result<EgoTurn, AcpClientError> {
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    run_turn_cancellable(state, root, prompt, receiver).await
}

async fn run_turn_cancellable(
    state: &AppState,
    root: PathBuf,
    prompt: String,
    mut cancellation: tokio::sync::watch::Receiver<bool>,
) -> Result<EgoTurn, AcpClientError> {
    if *cancellation.borrow() {
        return Err(AcpClientError::invalid_input("One-shot request cancelled"));
    }
    let config = ego_config(state)?;
    // Bounded by `INITIALIZE_TIMEOUT` inside the manager, which is why no
    // deadline is wrapped around it here: a timeout at this level would drop a
    // future that is not the one holding the ego child.
    let connection = state
        .acp
        .connect(
            &config,
            AcpConnectRequest {
                root: root.clone(),
                env: None,
            },
        )
        .await?;
    let connection_id = connection.connection_id;

    // The budget covers `session/new` and the prompt as well as the answer.
    // Around `collect` alone it bounded the part that was already the least
    // likely to hang, and left the caller's total unbounded.
    let outcome = tokio::select! {
        biased;
        _ = cancellation.wait_for(|cancelled| *cancelled) => {
            Err(AcpClientError::invalid_input("One-shot request cancelled"))
        }
        result = tokio::time::timeout(
        TURN_TIMEOUT,
        drive(
            state,
            &connection_id,
            connection.latest_sequence,
            root,
            prompt,
        ),
    ) => match result {
        Ok(outcome) => outcome,
        // `None` for the operation: a turn that ran out of time is not a
        // capability ego failed to offer, and naming one would send a reader
        // looking for a missing feature.
        Err(_) => Err(AcpClientError::agent_error(
            connection_id,
            None,
            format!(
                "ego did not finish the turn within {}s",
                TURN_TIMEOUT.as_secs()
            ),
        )),
        },
    };

    // The settlement is not reported: the caller asked for an answer, and a
    // connection that would not shut down cleanly says nothing about whether
    // the answer is good.
    let _ = state.acp.disconnect(connection_id).await;
    outcome
}

/// Everything between a live connection and an answer.
async fn drive(
    state: &AppState,
    connection_id: &AcpConnectionId,
    from_sequence: u64,
    root: PathBuf,
    prompt: String,
) -> Result<EgoTurn, AcpClientError> {
    let connection_id = *connection_id;
    // Subscribed before the session exists, from where the connect snapshot
    // ended: a subscription taken after the prompt would race the first chunks,
    // and one taken from zero would replay the connection's own startup.
    let mut events = state.acp.subscribe(connection_id, from_sequence + 1)?;

    let attachment = state
        .acp
        .new_unattended_session(
            connection_id,
            AcpSessionAuthority {
                cwd: root,
                additional_directories: Vec::new(),
                mcp_servers: Vec::new(),
            },
        )
        .await?;
    let session_id = attachment.session_id.clone();

    state
        .acp
        .prompt(
            connection_id,
            session_id.clone(),
            vec![v1::ContentBlock::Text(v1::TextContent::new(prompt))],
        )
        .await?;

    // No deadline here: `run_turn` wraps this whole function, so a budget at
    // this depth would bound the last step and leave the three before it free.
    let collected = collect(state, connection_id, &mut events, &session_id).await?;

    if let Some(message) = collected.failure() {
        return Err(
            AcpClientError::agent_error(connection_id, None, message.to_owned())
                .with_session_id(session_id),
        );
    }
    let mut turn = collected.finish().ok_or_else(|| {
        AcpClientError::transport_closed(connection_id).with_session_id(session_id.clone())
    })?;
    turn.session_id = Some(session_id.to_string());
    Ok(turn)
}

/// Read until the turn settles, refusing every question on the way.
///
/// The refusal is sent from inside this loop, not queued for afterwards: a
/// question is what *stops* the turn, so anything deferred until the turn ends
/// waits for something that will never happen. That is the hang this whole
/// module is shaped around.
async fn collect(
    state: &AppState,
    connection_id: AcpConnectionId,
    events: &mut super::events::AcpEventStream,
    session_id: &v1::SessionId,
) -> Result<TurnCollector, AcpClientError> {
    let mut collector = TurnCollector::default();

    while let Some(event) = events.recv().await {
        // A gap is terminal here for the same reason it is on the panel's
        // stream: the missing events are gone, and an answer assembled from
        // what survived would be a different answer with no sign of it.
        let envelope = event?;
        // Connection-wide events carry no session; everything else must be this
        // one's. A second session on this connection is not possible today, and
        // filtering rather than assuming keeps it that way if it becomes so.
        if envelope
            .session_id
            .as_ref()
            .is_some_and(|id| id != session_id)
        {
            continue;
        }

        match collector.observe(&envelope.event) {
            Step::Continue => {}
            Step::Refuse(refusal) => refuse(state, connection_id, refusal).await,
            Step::Done => break,
        }
    }

    Ok(collector)
}

/// Settle one request with the answer that grants nothing.
///
/// A failure is not propagated. Every way this can fail means the seat is
/// already gone — the turn was cancelled, ego withdrew the question, the
/// connection settled — and none of them is a reason to throw away an answer
/// the caller is still waiting for.
async fn refuse(state: &AppState, connection_id: AcpConnectionId, refusal: Refusal) {
    let _ = match refusal {
        Refusal::Permission(request_id) => state
            .acp
            .respond_permission(
                connection_id,
                request_id,
                v1::RequestPermissionOutcome::Cancelled,
            )
            .await
            .map(|_| ()),
        Refusal::Elicitation(request_id) => state
            .acp
            .respond_elicitation(connection_id, request_id, v1::ElicitationAction::Cancel)
            .await
            .map(|_| ()),
    };
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: AI Chat delegation instructions leaking into unrelated unattended Smart Prompts.
    #[tokio::test]
    async fn unattended_prompt_does_not_inherit_ai_chat_master_instructions() {
        let dir = tempfile::TempDir::new_in(tuic_test_support::test_temp_root()).unwrap();
        for (source, target) in [
            ("prompt-turn.jsonl", "scenario.jsonl"),
            ("ego-initialize.json", "ego-initialize.json"),
        ] {
            std::fs::copy(
                std::path::Path::new("tests/fixtures/acp").join(source),
                dir.path().join(target),
            )
            .unwrap();
        }
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!(
                "tuic-acp-fixture-agent{}",
                std::env::consts::EXE_SUFFIX
            ));
        assert!(
            executable.is_file(),
            "fixture agent must be built by nextest setup"
        );
        state.config.write().ego_executable = executable.to_string_lossy().into_owned();
        state.config.write().ai_chat_master_prompt =
            "Delegate all work to terminal agents; do not answer directly.".into();
        // The fixture refuses extra launch instructions and returns two recorded text chunks.
        let turn = run_prompt(
            &state,
            dir.path().to_owned(),
            "Transform selected text".into(),
        )
        .await
        .expect("AI Chat instructions must not alter an unattended launch");
        assert_eq!(turn.text, "onetwo");
        assert!(turn.session_id.as_ref().is_some_and(|id| !id.is_empty()));
        let wire = serde_json::to_value(&turn).unwrap();
        assert!(wire.get("sessionId").is_none());
        assert!(wire.get("session_id").is_none());
    }

    fn chunk(text: &str) -> AcpClientEvent {
        AcpClientEvent::SessionUpdate {
            update: Box::new(v1::SessionUpdate::AgentMessageChunk(v1::ContentChunk::new(
                v1::ContentBlock::Text(v1::TextContent::new(text.to_string())),
            ))),
        }
    }

    fn thought(text: &str) -> AcpClientEvent {
        AcpClientEvent::SessionUpdate {
            update: Box::new(v1::SessionUpdate::AgentThoughtChunk(v1::ContentChunk::new(
                v1::ContentBlock::Text(v1::TextContent::new(text.to_string())),
            ))),
        }
    }

    fn settled(stop_reason: v1::StopReason) -> AcpClientEvent {
        AcpClientEvent::TurnSettled {
            stop_reason,
            usage: None,
        }
    }

    #[test]
    fn a_failed_prompt_ends_an_unattended_turn_with_its_diagnostic() {
        let mut collector = TurnCollector::default();
        assert_eq!(
            collector.observe(&AcpClientEvent::TurnFailed {
                message: "no capabilities are configured".to_string(),
                state: crate::acp::AcpAttachmentState::Idle,
            }),
            Step::Done
        );
        assert_eq!(collector.failure(), Some("no capabilities are configured"));
    }

    // Catches: Cancel arriving before generation still allowing a new model process.
    #[test]
    fn one_shot_cancel_before_claim_revokes_the_reserved_handle() {
        let requests = OneShotRequests::default();
        let id = requests.reserve();
        assert!(requests.cancel(&id));
        assert!(requests.claim(&id).is_err());
        assert!(!requests.cancel(&id));
    }

    // Catches: cancelling one fill stopping a sibling or allowing the same reservation to launch twice.
    #[tokio::test]
    async fn one_shot_cancel_wakes_only_the_owned_claimed_turn() {
        let requests = OneShotRequests::default();
        let first = requests.reserve();
        let second = requests.reserve();
        let mut receiver = requests.claim(&first).unwrap();
        let sibling = requests.claim(&second).unwrap();
        assert!(requests.claim(&first).is_err());
        assert!(requests.cancel(&first));
        receiver.changed().await.unwrap();
        assert!(*receiver.borrow());
        assert!(!*sibling.borrow());
    }

    fn permission_asked(request_id: AcpHostRequestId) -> AcpClientEvent {
        AcpClientEvent::PermissionRequested {
            request_id,
            request: Box::new(v1::RequestPermissionRequest::new(
                v1::SessionId::from("s1"),
                v1::ToolCallUpdate::new("t1", v1::ToolCallUpdateFields::new()),
                Vec::new(),
            )),
        }
    }

    fn elicitation_asked(request_id: AcpHostRequestId) -> AcpClientEvent {
        AcpClientEvent::ElicitationRequested {
            request_id,
            request: Box::new(v1::CreateElicitationRequest::new(
                v1::ElicitationMode::Url(v1::ElicitationUrlMode::new(
                    v1::ElicitationSessionScope::new(v1::SessionId::from("s1")),
                    v1::ElicitationId::from("e1"),
                    "https://example.invalid",
                )),
                "which one?",
            )),
        }
    }

    /// The shutdown must survive the caller going away, because that is the
    /// case it exists for.
    ///
    /// `/acp/one-shot` runs under the router's `REQUEST_TIMEOUT`, and
    /// `tower_http` DROPS the handler future when it fires; a browser that
    /// disconnects mid-turn does the same. Awaited inline, everything after the
    /// drop point was skipped — the `disconnect` in [`run_turn`] above all —
    /// and nothing else stops an ego process: its supervisor is an independent
    /// task, and only settled connections are ever pruned.
    ///
    /// The 1 ms is the drop, not a deadline on the work: `timeout` polls the
    /// inner future before it sleeps, so the spawn always happens, and the
    /// assertion below it refuses to let this pass by finishing early. The 5 s
    /// is the "did this hang" bound, far above the 50 ms the body needs.
    #[tokio::test]
    async fn a_caller_that_goes_away_does_not_cancel_the_shutdown() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let shut_down = Arc::new(AtomicBool::new(false));
        let in_body = Arc::clone(&shut_down);
        let turn = detached(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            in_body.store(true, Ordering::SeqCst);
            Ok::<(), AcpClientError>(())
        });

        tokio::time::timeout(Duration::from_millis(1), turn)
            .await
            .expect_err("the body must still be running when the caller is dropped");
        assert!(
            !shut_down.load(Ordering::SeqCst),
            "the body finished before the drop, so this run proves nothing"
        );

        for _ in 0..500 {
            if shut_down.load(Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the dropped caller cancelled the shutdown");
    }

    #[test]
    fn the_answer_is_every_chunk_ego_said_in_order() {
        let mut collector = TurnCollector::default();
        for event in [
            chunk("Hello, "),
            chunk("world"),
            settled(v1::StopReason::EndTurn),
        ] {
            collector.observe(&event);
        }
        let turn = collector.finish().expect("the turn settled");
        assert_eq!(turn.text, "Hello, world");
        assert_eq!(turn.stop_reason, v1::StopReason::EndTurn);
        assert_eq!(turn.declined, 0);
    }

    #[test]
    fn surrounding_blank_space_is_not_part_of_the_answer() {
        let mut collector = TurnCollector::default();
        collector.observe(&chunk("\n\n  the answer  \n"));
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").text, "the answer");
    }

    #[test]
    fn reasoning_is_not_the_answer() {
        let mut collector = TurnCollector::default();
        // A thought pasted into a person's shell by `outputTarget` is the model
        // thinking out loud where the result was supposed to go.
        for event in [
            thought("let me think"),
            chunk("42"),
            settled(v1::StopReason::EndTurn),
        ] {
            collector.observe(&event);
        }
        assert_eq!(collector.finish().expect("settled").text, "42");
    }

    #[test]
    fn a_permission_request_is_refused_the_moment_it_arrives() {
        let request_id = AcpHostRequestId::new();
        let mut collector = TurnCollector::default();
        // Not `Continue`: a question is what stops the turn, so anything the
        // driver defers until the turn ends waits for something that cannot
        // happen.
        assert_eq!(
            collector.observe(&permission_asked(request_id)),
            Step::Refuse(Refusal::Permission(request_id))
        );
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").declined, 1);
    }

    #[test]
    fn an_elicitation_is_refused_on_its_own_seat() {
        let request_id = AcpHostRequestId::new();
        let mut collector = TurnCollector::default();
        // The two seats take different answers, and answering one on the other
        // leaves the real question waiting.
        assert_eq!(
            collector.observe(&elicitation_asked(request_id)),
            Step::Refuse(Refusal::Elicitation(request_id))
        );
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").declined, 1);
    }

    #[test]
    fn a_turn_with_nothing_to_say_still_reports_what_it_asked_for() {
        let mut collector = TurnCollector::default();
        collector.observe(&permission_asked(AcpHostRequestId::new()));
        collector.observe(&settled(v1::StopReason::Cancelled));
        let turn = collector.finish().expect("settled");
        // An empty answer and "the model wanted a tool this mode cannot grant"
        // look identical without this count. The caller says which one happened.
        assert_eq!(turn.text, "");
        assert_eq!(turn.declined, 1);
        assert_eq!(turn.stop_reason, v1::StopReason::Cancelled);
    }

    #[test]
    fn a_connection_that_died_ends_the_read() {
        let mut collector = TurnCollector::default();
        // Reading on would wait for a turn that can no longer be settled, which
        // is the timeout rather than the answer.
        assert_eq!(
            collector.observe(&AcpClientEvent::ConnectionState {
                state: AcpConnectionState::Failed
            }),
            Step::Done
        );
        assert!(collector.finish().is_none());
    }

    #[test]
    fn a_turn_that_never_settled_is_not_an_empty_answer() {
        let mut collector = TurnCollector::default();
        collector.observe(&chunk("half of a sen"));
        // The stream ended mid-turn. Reporting "" here would tell the caller
        // ego had nothing to say, which is the one thing it did not do.
        assert!(collector.finish().is_none());
    }

    fn turn(text: &str, declined: usize) -> EgoTurn {
        EgoTurn {
            text: text.to_string(),
            stop_reason: v1::StopReason::EndTurn,
            declined,
            session_id: None,
        }
    }

    #[test]
    fn an_object_is_found_however_it_was_introduced() {
        for answer in [
            r#"{"a":1}"#,
            "```json\n{\"a\":1}\n```",
            "Sure! Here it is:\n{\"a\":1}\nHope that helps.",
        ] {
            assert_eq!(extract_json(answer), r#"{"a":1}"#, "for {answer:?}");
        }
    }

    #[test]
    fn an_answer_with_no_object_is_handed_back_to_fail_loudly() {
        // Returning "" or "{}" here would turn "ego argued with the prompt"
        // into an empty list with nothing to show a person.
        assert_eq!(
            extract_json("  I would rather not.  "),
            "I would rather not."
        );
    }

    #[test]
    fn the_outermost_braces_win() {
        // Nested objects, and a trailing sentence after the close.
        assert_eq!(
            extract_json(r#"here: {"a":{"b":2}} done"#),
            r#"{"a":{"b":2}}"#
        );
    }

    #[test]
    fn a_refused_tool_is_named_rather_than_reported_as_silence() {
        // The one failure that looks like a bug and is not: the answer is empty
        // because the model wanted something this mode does not grant.
        let err = answer_text(turn("", 3)).unwrap_err();
        assert!(err.contains("3 tools"), "{err}");

        let one = answer_text(turn("", 1)).unwrap_err();
        assert!(one.contains("1 tool that"), "{one}");
    }

    #[test]
    fn an_empty_turn_is_an_error_and_never_an_empty_answer() {
        let err = answer_text(turn("", 0)).unwrap_err();
        assert!(err.contains("without saying anything"), "{err}");
    }

    #[test]
    fn a_turn_that_said_something_is_simply_that() {
        assert_eq!(answer_text(turn("hello", 0)).unwrap(), "hello");
        // Declined questions do not spoil an answer that still arrived.
        assert_eq!(answer_text(turn("hello", 2)).unwrap(), "hello");
    }

    #[test]
    fn a_quoted_answer_cannot_paste_a_transcript_into_a_toast() {
        let long = "x".repeat(MAX_QUOTED * 3);
        let shown = quoted(&long);

        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), MAX_QUOTED + 1);
        // A short answer is quoted whole, with nothing added.
        assert_eq!(quoted("short"), "short");
    }

    #[test]
    fn quoting_cuts_on_a_character_and_not_a_byte() {
        // `&text[..cut]` panics on a byte index inside a character, so the cut
        // has to come from `char_indices`. Every char here is 4 bytes.
        let long = "🙂".repeat(MAX_QUOTED * 2);
        assert_eq!(quoted(&long).chars().count(), MAX_QUOTED + 1);
    }
}
