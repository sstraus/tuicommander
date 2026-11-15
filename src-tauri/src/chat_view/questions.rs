//! Question ledger of the exact transcript of a terminal (see `agent_question`).
//!
//! Tailed like a chat `View`, but kept apart from it: the ledger is read while a
//! wait is live, whether a chat viewer is open or not.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use super::{Agent, TAIL_WINDOW_BYTES, claude, codex};
use crate::agent_question::{QuestionLedger, Row};
use crate::state::AppState;
use crate::transcript_tail::{read_appended, read_last_window};

/// One terminal's question tail.
pub(crate) struct QuestionTail {
    agent: Agent,
    /// Exact native session id: a Claude row of another session never counts.
    session: String,
    path: PathBuf,
    offset: u64,
    attached: bool,
    identity: Option<same_file::Handle>,
    ledger: QuestionLedger,
}

impl QuestionTail {
    fn new(agent: Agent, session: String, path: PathBuf) -> Self {
        Self {
            agent,
            session,
            path,
            offset: 0,
            attached: false,
            identity: None,
            ledger: QuestionLedger::default(),
        }
    }

    fn is_for(&self, agent: Agent, session: &str, path: &Path) -> bool {
        self.agent == agent && self.session == session && self.path == path
    }

    fn restart(&mut self) {
        self.offset = 0;
        self.attached = false;
        self.identity = None;
        self.ledger = QuestionLedger::default();
    }

    /// Read what the agent appended. A replaced or shrunk file starts over.
    fn advance(&mut self) -> std::io::Result<()> {
        let identity = same_file::Handle::from_path(&self.path)?;
        if self.identity.as_ref().is_some_and(|old| *old != identity) {
            self.restart();
        }
        self.identity = Some(identity);
        let appended = if self.attached {
            let appended = read_appended(&self.path, &mut self.offset)?;
            if appended.restarted {
                self.restart();
                return self.advance();
            }
            appended
        } else {
            let window = read_last_window(&self.path, &mut self.offset, TAIL_WINDOW_BYTES)?;
            self.attached = true;
            window
        };
        for line in appended.text.lines() {
            for row in rows(self.agent, &self.session, line) {
                self.ledger.absorb(row);
            }
        }
        Ok(())
    }
}

fn rows(agent: Agent, session: &str, line: &str) -> Vec<Row> {
    match agent {
        Agent::Claude => claude::question_rows(line, session),
        Agent::Codex => codex::question_rows(line),
        Agent::Grok | Agent::Ego => Vec::new(),
    }
}

/// The exact transcript of a Claude or Codex terminal: the Codex hook binding,
/// or the Claude pid registry. Discovery's newest-file heuristic never counts.
fn exact_source(state: &AppState, sid: &str) -> Option<(Agent, String, PathBuf)> {
    let agent_type = state
        .session_maps
        .session_states
        .get(sid)?
        .agent_type
        .clone();
    match agent_type.as_deref() {
        Some("codex") => {
            let path = crate::codex_binding::transcript(state, sid)?;
            let session = state
                .session_maps
                .session_states
                .get(sid)?
                .codex_binding
                .as_ref()?
                .session_id
                .clone();
            Some((Agent::Codex, session, path))
        }
        Some("claude") => {
            let cwd = state.session_maps.sessions.get(sid)?.lock().cwd.clone()?;
            let pid = crate::pty::session_leaf_pid(state, sid)?;
            let config = crate::agent_session::read_agent_env_overrides("claude", pid)
                .remove("CLAUDE_CONFIG_DIR");
            let (session, path) =
                crate::agent_session::claude_transcript_for_pid(pid, &cwd, config.as_deref())?;
            Some((Agent::Claude, session, path))
        }
        _ => None,
    }
}

/// The unresolved question of the exact transcript for `request`. Blocking:
/// reads the registry and the appended transcript bytes.
pub(crate) fn transcript_question(
    state: &AppState,
    sid: &str,
    request: Option<&str>,
) -> Option<String> {
    let Some((agent, session, path)) = exact_source(state, sid) else {
        state.chat_views.questions.lock().remove(sid);
        return None;
    };
    let tail = {
        let mut tails = state.chat_views.questions.lock();
        let tail = tails.entry(sid.to_owned()).or_insert_with(|| {
            Arc::new(Mutex::new(QuestionTail::new(
                agent,
                session.clone(),
                path.clone(),
            )))
        });
        if !tail.lock().is_for(agent, &session, &path) {
            *tail = Arc::new(Mutex::new(QuestionTail::new(agent, session, path)));
        }
        tail.clone()
    };
    let mut tail = tail.lock();
    if let Err(error) = tail.advance() {
        tracing::debug!(target: "chat_view", sid, %error, "Question transcript unreadable");
        return None;
    }
    tail.ledger.question(request).map(str::to_owned)
}

#[cfg(test)]
impl super::ChatViews {
    pub(crate) fn seed_question_tail(&self, session_id: &str) {
        let tail = QuestionTail::new(Agent::Claude, String::new(), PathBuf::new());
        self.questions
            .lock()
            .insert(session_id.to_owned(), Arc::new(Mutex::new(tail)));
    }

    pub(crate) fn has_question_tail(&self, session_id: &str) -> bool {
        self.questions.lock().contains_key(session_id)
    }
}
