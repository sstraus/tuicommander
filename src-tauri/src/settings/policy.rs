//! Host-issued origin and one-use approval authority. No stored values enter this pool.
use super::{catalogue::Catalogue, request::*, transaction};
#[path = "policy_context.rs"]
mod context;
#[path = "policy_patch.rs"]
mod patch;
#[path = "policy_scope.rs"]
mod scope;
pub use context::{Audit, Caller, Preview};
use parking_lot::Mutex;
pub(super) use patch::patch;
pub(crate) use scope::{
    app_config_requires_confirmation, stdio_added, tunnel_profile_requires_confirmation,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};
use tokio::time::Instant;

const TTL: Duration = Duration::from_secs(300);
const MAX_PENDING: usize = 128;

#[derive(Clone)]
struct Proposal {
    caller: context::CallerKey,
    changed_ids: Vec<String>,
    target: Target,
    store: String,
    fingerprint: String,
    expires: Instant,
    preview: Preview,
    showing: bool,
    approved: bool,
}
#[derive(Default)]
struct State {
    proposals: BTreeMap<String, Proposal>,
    audit: VecDeque<Audit>,
}
impl State {
    fn record(&mut self, proposal: &Proposal, outcome: &str) {
        self.audit.push_back(Audit {
            connection: proposal.caller.connection.clone(),
            origin: proposal.caller.origin,
            target: proposal.target.clone(),
            store: proposal.store.clone(),
            changed_ids: proposal.changed_ids.clone(),
            outcome: outcome.into(),
        });
        while self.audit.len() > 256 {
            self.audit.pop_front();
        }
    }
    fn remove_expired(&mut self) {
        let ids: Vec<_> = self
            .proposals
            .iter()
            .filter(|(_, p)| p.expires <= Instant::now())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(proposal) = self.proposals.remove(&id) {
                self.record(&proposal, "expired");
            }
        }
    }
}
#[derive(Default)]
pub struct Policy {
    state: Mutex<State>,
}

impl Policy {
    /// Validate, describe and bind an exposure proposal without retaining secret inputs.
    pub(super) fn authorize(
        &self,
        cat: &Catalogue,
        caller: &Caller,
        request: &Request,
        latest: &Value,
        approval: Option<&str>,
        validate: impl Fn(&Value) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        caller.check()?;
        let (target, store, revision, edits) = patch(request)?;
        let fingerprint = transaction::revision(&json!([target, store, revision, edits]))?;
        if let Some(id) = approval {
            let mut state = self.state.lock();
            let pending = state
                .proposals
                .remove(id)
                .ok_or_else(|| Error::new("settings", "confirmation_denied"))?;
            if pending.caller != caller.key
                || pending.fingerprint != fingerprint
                || pending.expires <= Instant::now()
                || !pending.approved
            {
                state.record(&pending, "confirmation_denied");
                return Err(Error::new("settings", "confirmation_denied"));
            }
            state.record(&pending, "approval_consumed");
            // Consume before revalidation: conflict/failure must never leave a reusable grant.
            drop(state);
            let prepared = transaction::prepare(cat, store, latest, revision, &edits)?;
            validate(&prepared.value)?;
            return Ok(());
        }
        let prepared = transaction::prepare(cat, store, latest, revision, &edits)?;
        validate(&prepared.value)?;
        let fields: Vec<_> = prepared
            .reply
            .changed_ids
            .iter()
            .filter_map(|id| cat.fields.get(id))
            .collect();
        if caller.owner_ui || !scope::adds_authority(&fields, latest, &prepared.value) {
            return Ok(());
        }
        let message = patch::preview(cat, store, &fields, &edits, latest, &prepared.value);
        let mut state = self.state.lock();
        state.remove_expired();
        if state.proposals.len() >= MAX_PENDING {
            return Err(Error::new("settings", "unavailable"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.proposals.insert(
            id.clone(),
            Proposal {
                caller: caller.key.clone(),
                changed_ids: prepared.reply.changed_ids.clone(),
                target: target.clone(),
                store: store.into(),
                fingerprint,
                expires: Instant::now() + TTL,
                preview: Preview {
                    title: "Approve settings change".into(),
                    message,
                    remaining: TTL,
                },
                showing: false,
                approved: false,
            },
        );
        let proposal = state.proposals[&id].clone();
        state.record(&proposal, "confirmation_required");
        let mut error = Error::new(store, "confirmation_required");
        error.proposal_id = Some(id);
        error.next_action = if caller.key.origin == "mcp" {
            "ui confirm proposalId"
        } else {
            "replay exact edit with approval=proposalId and confirm=true"
        }
        .into();
        Err(error)
    }

    /// Lease a proposal to the real dialog once. This never grants approval itself.
    pub(crate) fn begin_confirmation(&self, caller: &Caller, id: &str) -> Result<Preview, Failure> {
        caller.check()?;
        let mut state = self.state.lock();
        let pending = state
            .proposals
            .get_mut(id)
            .ok_or_else(|| Error::new("settings", "confirmation_denied"))?;
        if pending.caller != caller.key
            || pending.expires <= Instant::now()
            || pending.showing
            || pending.approved
        {
            return Err(Error::new("settings", "confirmation_denied"));
        }
        pending.showing = true;
        let mut preview = pending.preview.clone();
        preview.remaining = pending.expires.saturating_duration_since(Instant::now());
        Ok(preview)
    }
    /// Called only after the existing human-response channel resolves.
    pub(crate) fn finish_confirmation(
        &self,
        caller: &Caller,
        id: &str,
        approved: bool,
    ) -> Result<(), Failure> {
        let mut state = self.state.lock();
        let Some(mut pending) = state.proposals.remove(id) else {
            return Err(Error::new("settings", "confirmation_denied"));
        };
        if !approved
            || pending.caller != caller.key
            || pending.expires <= Instant::now()
            || !pending.showing
        {
            state.record(&pending, "confirmation_denied");
            return Err(Error::new("settings", "confirmation_denied"));
        }
        pending.approved = true;
        state.record(&pending, "approved");
        state.proposals.insert(id.to_owned(), pending);
        Ok(())
    }

    /// Teardown revokes pending and granted proposals for that protocol connection.
    pub fn disconnect_mcp(&self, connection: &str) {
        let mut state = self.state.lock();
        let ids: Vec<_> = state
            .proposals
            .iter()
            .filter(|(_, p)| p.caller.origin == "mcp" && p.caller.connection == connection)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(pending) = state.proposals.remove(&id) {
                state.record(&pending, "disconnected");
            }
        }
    }
    pub(crate) fn cancel_confirmation(&self, caller: &Caller, id: &str) {
        let mut state = self.state.lock();
        if state
            .proposals
            .get(id)
            .is_some_and(|p| p.caller == caller.key)
            && let Some(pending) = state.proposals.remove(id)
        {
            state.record(&pending, "cancelled");
        }
    }

    pub fn audit(&self) -> Vec<Audit> {
        self.state.lock().audit.iter().cloned().collect()
    }
}
