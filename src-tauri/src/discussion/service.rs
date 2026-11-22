//! The Discussion operations shared by Tauri IPC and HTTP.
//!
//! A question returns at once with its journal entry; the fork runs on a task
//! and its `answer` entry lands through `emit_progress_entry`, so the journal's
//! `progress-recorded` event, toast and unread state cover both transports.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use tokio::sync::oneshot;

use super::store::{self, NewQuestion, ThreadDetail};
use super::{Exchange, FORK_TIMEOUT, Turn};
use crate::progress::{ProgressEntry, ProgressStore};
use crate::state::AppState;

/// Forks one terminal may run at once.
pub(crate) const MAX_RUNNING_PER_TERMINAL: usize = 2;
/// Longest question accepted, in chars.
const MAX_QUESTION_CHARS: usize = 20_000;

/// Cancel handles of running turns, by thread id. Process-lifetime: a restart
/// kills every fork and `startup` answers their turns as failed.
static RUNNING: LazyLock<Mutex<HashMap<i64, oneshot::Sender<()>>>> =
    LazyLock::new(Default::default);

fn check_question(question: &str) -> Result<&str, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("the question is empty".to_owned());
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(format!(
            "the question is longer than {MAX_QUESTION_CHARS} characters"
        ));
    }
    Ok(question)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| format!("discussion task failed: {e}"))?
}

fn emit(state: &AppState, entry: ProgressEntry) {
    crate::mcp_http::mcp_transport::emit_progress_entry(state, entry);
}

/// Ask a side question about the agent running in `session_id`.
pub(crate) async fn ask(
    state: Arc<AppState>,
    session_id: String,
    question: String,
) -> Result<ProgressEntry, String> {
    let question = check_question(&question)?.to_owned();
    let st = state.clone();
    let session = session_id.clone();
    let (adapter, target, project, agent_name) = blocking(move || {
        let (adapter, target) = super::resolve_target(&st, &session)?;
        let project = crate::progress::project_for_session(&st, &session)
            .ok_or("this terminal is not inside a registered repository")?;
        // Filed under the owning project, like every other journal entry.
        let project = crate::progress::project_of(&project)?;
        let agent_name = st
            .session_maps
            .sessions
            .get(&session)
            .map(|s| s.lock().display_name.clone());
        Ok((adapter, target, project, agent_name.flatten()))
    })
    .await?;
    let (session, q) = (session_id.clone(), question.clone());
    let (entry, turn_id) = blocking(move || {
        let journal = ProgressStore::open()?;
        if store::running_count(&journal, &session)? >= MAX_RUNNING_PER_TERMINAL {
            return Err(format!(
                "{MAX_RUNNING_PER_TERMINAL} questions are already running for this terminal; wait for one or cancel it"
            ));
        }
        let new = NewQuestion { project: &project, pty_id: &session, agent_name: agent_name.as_deref(), question: &q };
        store::ask(&journal, &new)
    })
    .await?;
    emit(&state, entry.clone());
    spawn_turn(
        state,
        entry.id,
        turn_id,
        adapter,
        target,
        Turn::NewFork,
        question,
    );
    Ok(entry)
}

/// Continue a thread's fork. `entry_id` is any question or answer of the
/// thread. The agent must still run in the thread's terminal: its env and
/// binary are read from the live process.
pub(crate) async fn follow_up(
    state: Arc<AppState>,
    entry_id: i64,
    question: String,
) -> Result<ProgressEntry, String> {
    let question = check_question(&question)?.to_owned();
    let thread = get(entry_id).await?;
    let pty = thread.pty_id.clone().ok_or("this thread has no terminal")?;
    let st = state.clone();
    // DEFERRED (2026-11-21) — a follow-up after the agent terminal closed needs
    // the fork's binary, config dir and model stored with the thread.
    let (adapter, target) = blocking(move || super::resolve_target(&st, &pty))
        .await
        .map_err(|e| format!("cannot continue this thread: {e}"))?;
    let (thread_id, q) = (thread.thread_id, question.clone());
    let (entry, turn_id, fork_session_id) =
        blocking(move || store::follow_up(&ProgressStore::open()?, thread_id, &q)).await?;
    emit(&state, entry.clone());
    let prior = thread.turns.iter().filter_map(exchange).collect();
    spawn_turn(
        state,
        thread.thread_id,
        turn_id,
        adapter,
        target,
        Turn::FollowUp {
            fork_session_id,
            prior,
        },
        question,
    );
    Ok(entry)
}

/// An answered turn as a follow-up prompt replays it; a failed turn has no answer.
fn exchange(turn: &store::TurnRecord) -> Option<Exchange> {
    let body = turn.answer.as_deref()?;
    let answer = format!("{}\n{body}", turn.title.as_deref().unwrap_or_default())
        .trim()
        .to_owned();
    Some(Exchange {
        question: turn.question.clone(),
        answer,
    })
}

fn spawn_turn(
    state: Arc<AppState>,
    thread_id: i64,
    turn_id: i64,
    adapter: Box<dyn super::Adapter>,
    target: super::ForkTarget,
    turn: Turn,
    question: String,
) {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    RUNNING.lock().insert(thread_id, cancel_tx);
    tokio::spawn(async move {
        let result = super::run(
            adapter.as_ref(),
            &target,
            &turn,
            &question,
            cancel_rx,
            FORK_TIMEOUT,
        )
        .await;
        RUNNING.lock().remove(&thread_id);
        if let Err(error) = &result {
            tracing::warn!(target: "discussion", thread_id, %error, "Discussion turn failed");
        }
        match blocking(move || store::finish(&ProgressStore::open()?, turn_id, &result)).await {
            Ok(entry) => emit(&state, entry),
            Err(error) => {
                tracing::error!(target: "discussion", thread_id, %error, "Cannot save a Discussion answer")
            }
        }
    });
}

pub(crate) async fn get(entry_id: i64) -> Result<ThreadDetail, String> {
    blocking(move || store::get(&ProgressStore::open()?, entry_id)).await
}

/// Stop a running turn. The task answers the thread as cancelled.
pub(crate) async fn cancel(entry_id: i64) -> Result<(), String> {
    let thread = get(entry_id).await?;
    let sender = RUNNING
        .lock()
        .remove(&thread.thread_id)
        .ok_or("this thread is not answering")?;
    let _ = sender.send(());
    Ok(())
}

/// At startup no fork survives: open turns are answered as failed.
pub(crate) fn startup() {
    match ProgressStore::open().and_then(|journal| store::fail_interrupted(&journal)) {
        Ok(0) => {}
        Ok(n) => {
            tracing::info!(target: "discussion", n, "Discussion turns interrupted by the restart")
        }
        Err(error) => {
            tracing::warn!(target: "discussion", %error, "Cannot close interrupted Discussion turns")
        }
    }
}
