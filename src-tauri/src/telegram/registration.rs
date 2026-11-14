use super::{Error, mail::PendingMail, runtime::Runtime};
use std::sync::Arc;

pub(super) struct Registration {
    pub peer: String,
    pub sid: String,
    pub pty: Option<String>,
    pub lifetime: Option<Arc<()>>,
}

impl Runtime {
    pub(super) fn registered_peer(&self) -> Option<&str> {
        let registration = self.registration.as_ref()?;
        let bound = self.state.mcp.to_session.get(&registration.sid)?;
        if self
            .state
            .resolve_peer_ref_checked(bound.value())
            .ok()
            .flatten()
            .as_deref()
            != Some(registration.peer.as_str())
            || !self.state.peer_agents.contains_key(&registration.peer)
        {
            return None;
        }
        let pty = self.state.live_pty_for_peer(&registration.peer);
        if registration
            .pty
            .as_ref()
            .is_some_and(|original| pty.as_ref() != Some(original))
        {
            return None;
        }
        if let Some(pty) = pty
            && (self.state.session_maps.exit_codes.contains_key(&pty)
                || self
                    .state
                    .session_state_with_shell(&pty)
                    .is_none_or(|s| s.agent_type.is_none()))
        {
            return None;
        }
        if let Some(pty) = &registration.pty {
            let session = self.state.session_maps.session_states.get(pty)?;
            if !session
                .telegram_registration_lifetime
                .as_ref()
                .zip(registration.lifetime.as_ref())
                .is_some_and(|(current, original)| Arc::ptr_eq(current, original))
            {
                return None;
            }
        }
        Some(&registration.peer)
    }

    pub(super) fn recipient_peer(&self) -> Option<String> {
        if let Some(peer) = self.registered_peer() {
            return Some(peer.to_owned());
        }
        let peers = crate::acp_chat::registered_ai_chat_peers(&self.state);
        self.state.acp.latest_live_peer(&peers)
    }

    pub(super) fn clear_registration(&mut self) {
        self.registration = None;
        super::settings::registration_status(&self.outbound.paths, None);
        self.pending.clear();
        for request in &mut self.requests {
            if request.completed.is_none() {
                request.failed = true;
            }
        }
        self.outbound.active = None;
        self.callbacks.clear();
    }

    pub(super) fn retire_registration(&mut self) {
        if self.registration.is_some() && self.registered_peer().is_none() {
            self.clear_registration();
        }
    }

    pub(super) async fn register(&mut self, caller: &str, sid: &str) -> Result<(), Error> {
        let bound = self
            .state
            .mcp
            .to_session
            .get(sid)
            .ok_or(Error::State)?
            .value()
            .clone();
        if self
            .state
            .resolve_peer_ref_checked(&bound)
            .map_err(|_| Error::State)?
            .as_deref()
            != Some(caller)
        {
            return Err(Error::State);
        }
        let mut candidate = Registration {
            peer: caller.into(),
            sid: sid.into(),
            pty: self.state.live_pty_for_peer(caller),
            lifetime: None,
        };
        if let Some(pty) = &candidate.pty
            && (self.state.session_maps.exit_codes.contains_key(pty)
                || self
                    .state
                    .session_state_with_shell(pty)
                    .is_none_or(|s| s.agent_type.is_none()))
        {
            return Err(Error::State);
        }
        if let Some(pty) = &candidate.pty {
            let mut session = self
                .state
                .session_maps
                .session_states
                .get_mut(pty)
                .ok_or(Error::State)?;
            // Serialize opt-in with the producer that observes foreground exit.
            if session.agent_type.is_none() {
                return Err(Error::State);
            }
            candidate.lifetime = Some(
                session
                    .telegram_registration_lifetime
                    .get_or_insert_with(|| Arc::new(()))
                    .clone(),
            );
        }
        if let Some(previous) = self.registered_peer().filter(|peer| *peer != caller) {
            let notice = PendingMail {
                id: format!(
                    "tg:{}:registration:{}",
                    self.config.bot_alias,
                    uuid::Uuid::new_v4()
                ),
                recipient: previous.into(),
                content: "Your Telegram registration was replaced by another agent.".into(),
            };
            super::native::offer(&self.state, &self.adapter_sid, &notice).await?;
        }
        if self.registered_peer() != Some(caller) {
            self.clear_registration();
        }
        self.registration = Some(candidate);
        let name = self
            .state
            .peer_agents
            .get(caller)
            .map(|peer| peer.name.clone());
        super::settings::registration_status(&self.outbound.paths, name);
        Ok(())
    }
}
