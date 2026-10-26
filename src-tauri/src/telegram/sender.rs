use super::{Error, runtime::Runtime};
use serde_json::Value;
use std::time::Duration;
use tokio::time::Instant;

const SEND_INTERVAL: Duration = Duration::from_secs(30);

impl Runtime {
    fn live_sender(&self, peer: &str) -> bool {
        if !self.state.peer_agents.contains_key(peer) {
            return false;
        }
        if let Some(pty) = self.state.live_pty_for_peer(peer) {
            return !self.state.session_maps.exit_codes.contains_key(&pty)
                && self
                    .state
                    .session_state_with_shell(&pty)
                    .is_some_and(|s| s.agent_type.is_some());
        }
        // Closed managed PTYs can retain their session snapshot briefly; they
        // are not external/headerless peers just because the live PTY is gone.
        if self.state.session_maps.session_states.contains_key(peer) {
            return false;
        }
        self.state
            .mcp
            .to_session
            .iter()
            .any(|binding| binding.value() == peer)
    }

    pub(super) fn sender_prefix(&self, caller: &str) -> Result<String, Error> {
        if !self.live_sender(caller) {
            return Err(Error::NotRegistered);
        }
        let peer = self.state.peer_agents.get(caller).ok_or(Error::State)?;
        let name = &peer.name;
        if name.trim().is_empty()
            || name.encode_utf16().count() > 128
            || name
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '[' | ']'))
        {
            return Err(Error::InvalidSenderName);
        }
        Ok(format!("[{name}] "))
    }

    pub(super) fn limit_sender(&mut self, caller: &str) -> Result<(), Error> {
        let now = Instant::now();
        // Only retain active intervals: memory follows recent senders, not history.
        self.sender_limits
            .retain(|_, sent| now.duration_since(*sent) < SEND_INTERVAL);
        if self.sender_limits.contains_key(caller) {
            return Err(Error::SenderRateLimited);
        }
        // Charge before I/O: an uncertain delivery must not enable a retry loop.
        self.sender_limits.insert(caller.into(), now);
        Ok(())
    }

    pub(super) fn reply_recipient(&self, envelope: &Value) -> Option<String> {
        let text = envelope["reply_to_text"].as_str()?;
        let (name, _) = text.strip_prefix('[')?.split_once("] ")?;
        let candidates: Vec<_> = self
            .state
            .peer_agents
            .iter()
            .filter(|peer| peer.name == name)
            .map(|peer| peer.key().clone())
            .collect();
        let mut matches = candidates.into_iter().filter(|peer| self.live_sender(peer));
        let peer = matches.next()?;
        // Exact names only; never let a UUID, prefix, alias or ambiguity guess.
        matches.next().is_none().then_some(peer)
    }
}
