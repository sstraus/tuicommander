//! Backend-owned auto-retry after a server-side API error (story 2746-2348).
//!
//! The retry budget, timer and submit live here so a session with no mounted
//! Terminal (mobile companion, headless) retries like the desktop. The frontend
//! no longer runs a retry of its own: one owner means no double submit.
//!
//! Lifecycle, all on `SessionState::api_retry`:
//! - `note_api_error` arms one timer per error, at most `RETRY_DELAYS.len()`
//!   times until a recovered turn.
//! - `note_working` (a busy status line) cancels a parked timer, keeps the budget.
//! - `note_idle` after observed work resets the budget: the turn recovered.
//! - The timer fires only if the same arming is still current. A shell that
//!   still reads busy is rechecked every `BUSY_RECHECK` instead of dropping the
//!   attempt (story 2717-f1c1); an idle shell gets `continue` through the
//!   Ink-safe agent writer.

use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];

/// How long a due retry waits before it rechecks a session that still reads busy.
pub(crate) const BUSY_RECHECK: Duration = Duration::from_secs(5);

/// What a due timer did: finished (submitted or dropped), or wait and recheck.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fired {
    Done,
    Recheck,
}

#[derive(Default, Clone, Debug)]
pub(crate) struct ApiRetryState {
    count: usize,
    work_observed: bool,
    pending: bool,
    generation: u64,
}

/// What an error decided: the delay to wait and the arming that owns the timer.
pub(crate) struct Armed {
    pub(crate) delay: Duration,
    pub(crate) generation: u64,
    pub(crate) attempt: usize,
}

/// Decide whether this error arms a retry. Pure state transition, no timer.
pub(crate) fn arm_after_error(
    state: &AppState,
    session_id: &str,
    error_kind: &str,
    enabled: bool,
) -> Option<Armed> {
    let mut session = state.session_maps.session_states.get_mut(session_id)?;
    session.api_retry.work_observed = false;
    if error_kind != "server" || !enabled || session.agent_type.is_none() {
        return None;
    }
    let retry = &mut session.api_retry;
    if retry.pending {
        return None;
    }
    if retry.count >= RETRY_DELAYS.len() {
        tracing::warn!(
            session_id,
            "[AutoRetry] exhausted {} retries, manual intervention needed",
            RETRY_DELAYS.len()
        );
        return None;
    }
    let delay = RETRY_DELAYS[retry.count];
    retry.count += 1;
    retry.pending = true;
    retry.generation += 1;
    Some(Armed {
        delay,
        generation: retry.generation,
        attempt: retry.count,
    })
}

/// Arm the retry for an `api-error` event and start its timer.
pub(crate) fn note_api_error(state: &Arc<AppState>, session_id: &str, error_kind: &str) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let enabled = agent_type.as_deref().is_some_and(|agent| {
        crate::config::load_agents_config()
            .agents
            .get(agent)
            .is_some_and(|settings| settings.auto_retry_on_error)
    });
    let Some(armed) = arm_after_error(state, session_id, error_kind, enabled) else {
        return;
    };
    tracing::info!(
        session_id,
        "[AutoRetry] attempt {}/{} in {}s",
        armed.attempt,
        RETRY_DELAYS.len(),
        armed.delay.as_secs()
    );
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(armed.delay);
        let mut rechecks = 0;
        while fire(&state, &session_id, armed.generation, rechecks) == Fired::Recheck {
            rechecks += 1;
            std::thread::sleep(BUSY_RECHECK);
        }
    });
}

/// Timer body: submit `continue` if this arming is current and the agent is idle.
/// A busy shell has not settled after the error yet: keep the arming and ask for
/// a recheck. Working evidence or a recovered turn cancels it. `rechecks` counts
/// earlier rechecks of this attempt, so only the first one logs at info.
pub(crate) fn fire(
    state: &Arc<AppState>,
    session_id: &str,
    generation: u64,
    rechecks: usize,
) -> Fired {
    let shell = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|shell| shell.load(std::sync::atomic::Ordering::Acquire));
    {
        let Some(mut session) = state.session_maps.session_states.get_mut(session_id) else {
            tracing::info!(session_id, "[AutoRetry] skipped attempt, no live session");
            return Fired::Done;
        };
        let retry = &mut session.api_retry;
        if !retry.pending || retry.generation != generation {
            return Fired::Done;
        }
        if shell == Some(crate::pty::SHELL_BUSY) {
            let attempt = retry.count;
            if rechecks == 0 {
                tracing::info!(
                    session_id,
                    "[AutoRetry] session still busy, rechecking in {}s before \"continue\" (attempt {attempt})",
                    BUSY_RECHECK.as_secs()
                );
            } else {
                tracing::debug!(
                    session_id,
                    "[AutoRetry] session still busy, rechecking in {}s before \"continue\" (attempt {attempt})",
                    BUSY_RECHECK.as_secs()
                );
            }
            return Fired::Recheck;
        }
        retry.pending = false;
        // A refused turn (OSC 7770 `state=error`) repeats on every "continue".
        if session.agent_type.is_none() || session.agent_error {
            return Fired::Done;
        }
    }
    if shell != Some(crate::pty::SHELL_IDLE) {
        return Fired::Done;
    }
    // A draft in the composer would be erased by the writer's Ctrl-U.
    let drafting = state
        .session_maps
        .input_buffers
        .get(session_id)
        .is_some_and(|buffer| !buffer.lock().content().is_empty());
    if drafting {
        tracing::info!(
            session_id,
            "[AutoRetry] skipped attempt, a draft is in the composer"
        );
        return Fired::Done;
    }
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    crate::pty::spawn_injection_job(move || {
        tracing::info!(session_id = %session_id, "[AutoRetry] injecting \"continue\"");
        match crate::pty::write_agent_command_to_pty(&state, &session_id, "continue") {
            // The writer leaves bookkeeping to the caller; recording the line
            // reopens the parser's api-error dedup so a failed retry notifies.
            Ok(()) => crate::pty::record_submitted_line(&state, &session_id, "continue".into(), -1),
            Err(error) => {
                tracing::error!(session_id = %session_id, "[AutoRetry] failed to write: {error}");
            }
        }
    });
    Fired::Done
}

/// Working evidence: cancel a parked timer, keep the budget.
pub(crate) fn note_working(state: &AppState, session_id: &str) {
    if let Some(mut session) = state.session_maps.session_states.get_mut(session_id) {
        if session.api_retry.pending {
            tracing::info!(
                session_id,
                "[AutoRetry] parked retry cancelled, the agent reports work"
            );
        }
        session.api_retry.pending = false;
        session.api_retry.generation += 1;
        session.api_retry.work_observed = true;
    }
}

/// The turn reached idle. Reset the budget only if work followed the last error.
pub(crate) fn note_idle(state: &AppState, session_id: &str) {
    if let Some(mut session) = state.session_maps.session_states.get_mut(session_id)
        && session.api_retry.work_observed
    {
        session.api_retry = ApiRetryState {
            generation: session.api_retry.generation + 1,
            ..Default::default()
        };
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::pty::{SHELL_BUSY, SHELL_IDLE, wait_for_injection_queue};
    use crate::test_support::{agent_session, insert_recording_session};

    fn session(shell: u8) -> (Arc<AppState>, &'static str, Arc<std::sync::Mutex<Vec<u8>>>) {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "api-retry";
        let written = insert_recording_session(&state, sid);
        agent_session(&state, sid, shell);
        (state, sid, written)
    }

    fn typed(written: &Arc<std::sync::Mutex<Vec<u8>>>) -> String {
        wait_for_injection_queue();
        String::from_utf8_lossy(&written.lock().unwrap()).into_owned()
    }

    /// Catches: no retry without a mounted Terminal. This session has no frontend
    /// at all; the backend alone must submit `continue` after a server error.
    #[test]
    fn headless_session_retries_after_a_server_error() {
        let (state, sid, written) = session(SHELL_IDLE);
        let armed = arm_after_error(&state, sid, "server", true).expect("armed");
        assert_eq!((armed.delay, armed.attempt), (RETRY_DELAYS[0], 1));
        fire(&state, sid, armed.generation, 0);
        let out = typed(&written);
        assert!(
            out.contains("continue") && out.contains('\r'),
            "wrote {out:?}"
        );
    }

    /// Catches: auto-retry typing `continue` into an agent whose every turn is
    /// refused (OSC 7770 `state=error`), the wake loop of #3011-1784.
    #[test]
    fn a_refused_turn_cancels_the_submit() {
        let (state, sid, written) = session(SHELL_IDLE);
        let armed = arm_after_error(&state, sid, "server", true).expect("armed");
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_error = true;
        assert_eq!(fire(&state, sid, armed.generation, 0), Fired::Done);
        assert_eq!(typed(&written), "");
    }

    /// Catches: retrying auth errors, or retrying while the setting is off.
    #[test]
    fn only_enabled_server_errors_arm() {
        let (state, sid, _) = session(SHELL_IDLE);
        assert!(arm_after_error(&state, sid, "auth", true).is_none());
        assert!(arm_after_error(&state, sid, "unknown", true).is_none());
        assert!(arm_after_error(&state, sid, "server", false).is_none());
    }

    /// Catches: an unbounded retry loop, and a second timer racing the first.
    #[test]
    fn budget_is_three_with_the_desktop_delays_and_one_timer_at_a_time() {
        let (state, sid, _) = session(SHELL_IDLE);
        let mut delays = vec![];
        for _ in 0..3 {
            let armed = arm_after_error(&state, sid, "server", true).expect("armed");
            assert!(
                arm_after_error(&state, sid, "server", true).is_none(),
                "pending"
            );
            delays.push(armed.delay);
            fire(&state, sid, armed.generation, 0);
        }
        assert_eq!(delays, RETRY_DELAYS.to_vec());
        assert!(
            arm_after_error(&state, sid, "server", true).is_none(),
            "exhausted"
        );
    }

    /// Catches: a recovered turn still counted against the budget forever.
    #[test]
    fn idle_after_work_resets_the_budget_but_idle_alone_does_not() {
        let (state, sid, _) = session(SHELL_IDLE);
        let armed = arm_after_error(&state, sid, "server", true).unwrap();
        fire(&state, sid, armed.generation, 0);
        note_idle(&state, sid);
        assert_eq!(
            arm_after_error(&state, sid, "server", true)
                .unwrap()
                .attempt,
            2
        );
        note_working(&state, sid);
        note_idle(&state, sid);
        assert_eq!(
            arm_after_error(&state, sid, "server", true)
                .unwrap()
                .attempt,
            1
        );
    }

    /// Catches: the timer firing while the user types a reply in the composer —
    /// the Ctrl-U of the agent writer erases the draft and `continue` replaces it.
    #[test]
    fn a_draft_in_the_composer_cancels_the_submit() {
        let (state, sid, written) = session(SHELL_IDLE);
        let armed = arm_after_error(&state, sid, "server", true).unwrap();
        let mut draft = crate::input_line_buffer::InputLineBuffer::new();
        draft.feed("please check the logs first");
        state
            .session_maps
            .input_buffers
            .insert(sid.to_string(), parking_lot::Mutex::new(draft));
        fire(&state, sid, armed.generation, 0);
        assert!(
            typed(&written).is_empty(),
            "the user's draft was overwritten"
        );
    }

    /// Catches: `continue` typed over an agent that already resumed.
    #[test]
    fn working_evidence_or_a_busy_shell_cancels_the_submit() {
        let (state, sid, written) = session(SHELL_IDLE);
        let armed = arm_after_error(&state, sid, "server", true).unwrap();
        note_working(&state, sid);
        fire(&state, sid, armed.generation, 0);
        assert_eq!(typed(&written), "");
    }

    /// Catches: a retry that falls due while the session still reads busy is
    /// dropped, so the agent stays stopped (2717-f1c1, seen 2026-11-02).
    #[test]
    fn a_busy_shell_rechecks_and_submits_once_it_settles() {
        let (state, sid, written) = session(SHELL_BUSY);
        let armed = arm_after_error(&state, sid, "server", true).unwrap();
        assert_eq!(fire(&state, sid, armed.generation, 0), Fired::Recheck);
        assert_eq!(fire(&state, sid, armed.generation, 1), Fired::Recheck);
        assert!(
            typed(&written).is_empty(),
            "continue typed over a busy agent"
        );
        assert!(
            arm_after_error(&state, sid, "server", true).is_none(),
            "a second timer armed while the first one rechecks"
        );
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .store(SHELL_IDLE, std::sync::atomic::Ordering::Release);
        assert_eq!(fire(&state, sid, armed.generation, 2), Fired::Done);
        let out = typed(&written);
        assert_eq!(out.matches("continue").count(), 1, "wrote {out:?}");
    }

    /// Catches: working evidence during a busy recheck lets the loop submit anyway.
    #[test]
    fn working_evidence_ends_a_busy_recheck() {
        let (state, sid, written) = session(SHELL_BUSY);
        let armed = arm_after_error(&state, sid, "server", true).unwrap();
        assert_eq!(fire(&state, sid, armed.generation, 0), Fired::Recheck);
        note_working(&state, sid);
        assert_eq!(fire(&state, sid, armed.generation, 1), Fired::Done);
        assert_eq!(typed(&written), "");
    }
}
