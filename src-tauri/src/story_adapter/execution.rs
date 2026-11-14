//! TUIC execution state over an external story (story 2730-07ba).
//!
//! The adapter's five statuses carry the content status. The native ledger adds,
//! keyed by adapter id + story id, what the protocol has no field for: the claim,
//! review and QA as sub-states of the adapter's `in_progress`, the transition
//! history, and the integration receipt that a TUIC approval writes. A story the
//! adapter reports `complete` without that receipt was completed outside TUIC: it
//! shows as such and releases no dependent.

use super::{ApplyOp, ExternalStories, ExternalStory, revision, status_of, text, unsupported};
use crate::stories::{
    AvailableTransitions, ExternalExecution, ExternalTransition, Story, StoryCommand, StoryOrigin,
    StoryPhaseMove, StoryStatus, StoryStore, StoryTransition, StoryTransitionActor,
    evaluate_story_command, only_manual_pending,
};
use std::collections::HashMap;

/// The ledger rows of one adapter, read once per call.
pub(super) struct Ledger {
    adapter_id: String,
    store: StoryStore,
    executions: HashMap<String, ExternalExecution>,
    /// Origin and file scope of proposal stories: the protocol has no field for them.
    shapes: HashMap<String, (StoryOrigin, Vec<String>)>,
}

impl Ledger {
    pub(super) fn store(&self) -> &StoryStore {
        &self.store
    }

    pub(super) fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    fn execution(&self, id: &str) -> ExternalExecution {
        self.executions.get(id).cloned().unwrap_or_default()
    }

    /// Done in TUIC terms: complete in the adapter and approved through TUIC.
    fn accepted(&self, raw: &ExternalStory) -> bool {
        raw.status == "complete" && self.execution(&raw.id).receipt_version.is_some()
    }

    pub(super) fn completed_outside(&self, raw: &ExternalStory) -> bool {
        raw.status == "complete" && !self.accepted(raw)
    }

    /// The story as TUIC shows it. `dependencies_met` holds when every
    /// dependency is accepted; an adapter `ready` story without it is backlog.
    pub(super) fn overlay(
        &self,
        raw: ExternalStory,
        dependencies_met: bool,
    ) -> Result<Story, String> {
        let execution = self.execution(&raw.id);
        let status = match raw.status.as_str() {
            "in_progress" => execution.sub_state.unwrap_or(StoryStatus::InProgress),
            "ready" if !dependencies_met => StoryStatus::Backlog,
            other => status_of(other)?,
        };
        let claim_session = matches!(status, StoryStatus::InProgress | StoryStatus::Review)
            .then(|| execution.claim_session.clone())
            .flatten();
        let (origin, file_scope) = self
            .shapes
            .get(&raw.id)
            .cloned()
            .unwrap_or((StoryOrigin::Native, Vec::new()));
        Ok(Story {
            archived: false,
            status,
            revision: overlay_revision(&raw.version, &execution),
            checked: raw.criteria.iter().map(|c| c.checked).collect(),
            criteria: raw.criteria.into_iter().map(|c| c.text).collect(),
            id: raw.id,
            plan_id: raw.plan_id,
            title: raw.title,
            dependencies: raw.dependencies,
            priority: raw.priority.unwrap_or(2),
            story_type: None,
            origin,
            file_scope,
            claim_session,
            problem: (!raw.problem.is_empty()).then_some(raw.problem),
            risk: None,
        })
    }
}

/// The revision covers the adapter version and the TUIC execution state, so a
/// sub-state change alone is also caught by the expected-revision check.
pub(super) fn overlay_revision(version: &str, execution: &ExternalExecution) -> i64 {
    revision(&format!(
        "{version}\n{:?}\n{:?}\n{:?}",
        execution.sub_state, execution.claim_session, execution.receipt_version
    ))
}

fn adapter_status(status: StoryStatus) -> Result<&'static str, String> {
    Ok(match status {
        StoryStatus::Backlog => "pending",
        StoryStatus::Ready => "ready",
        StoryStatus::InProgress | StoryStatus::Review | StoryStatus::Qa => "in_progress",
        StoryStatus::Blocked => "blocked",
        StoryStatus::Done => "complete",
        StoryStatus::WontFix => return Err(unsupported("wont_fix")),
    })
}

/// The read state of one story: the raw record, its TUIC view and the ledger.
pub(super) struct Current {
    pub(super) raw: ExternalStory,
    story: Story,
    ledger: Ledger,
}

impl ExternalStories {
    pub(super) fn ledger(&self) -> Result<Ledger, String> {
        let adapter_id = self.backend.info().map_err(text)?.adapter.id;
        let store = crate::stories::execution_ledger()?;
        let executions = store.external_executions(&adapter_id)?;
        let shapes = store.external_proposal_shapes(&adapter_id)?;
        Ok(Ledger {
            adapter_id,
            store,
            executions,
            shapes,
        })
    }

    /// TUIC views of `raws`. A dependency outside `raws` is read from the adapter.
    pub(super) fn overlay_all(
        &self,
        raws: Vec<ExternalStory>,
        ledger: &Ledger,
    ) -> Result<Vec<Story>, String> {
        let by_id: HashMap<String, ExternalStory> = raws
            .iter()
            .map(|raw| (raw.id.clone(), raw.clone()))
            .collect();
        raws.into_iter()
            .map(|raw| {
                let met = self.dependencies_met(&raw.dependencies, &by_id, ledger)?;
                ledger.overlay(raw, met)
            })
            .collect()
    }

    fn dependencies_met(
        &self,
        dependencies: &[String],
        known: &HashMap<String, ExternalStory>,
        ledger: &Ledger,
    ) -> Result<bool, String> {
        for id in dependencies {
            let accepted = match known.get(id) {
                Some(raw) => ledger.accepted(raw),
                None => ledger.accepted(&self.backend.get_story(id).map_err(text)?),
            };
            if !accepted {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn read_current(&self, id: &str, expected_revision: i64) -> Result<Current, String> {
        let ledger = self.ledger()?;
        let raw = self.backend.get_story(id).map_err(text)?;
        let mut views = self.overlay_all(vec![raw.clone()], &ledger)?;
        let story = views.pop().expect("one story in, one out");
        if story.revision != expected_revision {
            return Err(format!(
                "stale story revision: expected {expected_revision}, current {}",
                story.revision
            ));
        }
        Ok(Current { raw, story, ledger })
    }

    /// Writes `after` to the adapter (criterion checks, then the status), then
    /// the TUIC part to the ledger. The adapter write is not atomic with the
    /// ledger write: a ledger failure leaves the adapter changed and is reported.
    fn commit(
        &self,
        current: Current,
        after: Story,
        command: StoryCommand,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        let Current {
            raw,
            story: before,
            ledger,
        } = current;
        if after.criteria != before.criteria {
            return Err(unsupported("editing criteria"));
        }
        let mut ops = Vec::new();
        for (index, (was, now)) in before.checked.iter().zip(&after.checked).enumerate() {
            match (was, now) {
                (false, true) => ops.push(ApplyOp::CheckCriterion { index }),
                (true, false) => return Err(unsupported("uncheck_criterion")),
                _ => {}
            }
        }
        let status = adapter_status(after.status)?;
        if status != raw.status {
            ops.push(ApplyOp::SetStatus {
                status: status.into(),
            });
        }
        let expected = ledger.execution(&raw.id);
        // A claim on a story the adapter completed (or moved) outside TUIC is
        // stale: it holds nothing, and the same transaction clears it.
        let mut release_claims = Vec::new();
        if let Some(session) = after.claim_session.as_deref() {
            for (id, other) in &ledger.executions {
                if *id == raw.id || other.claim_session.as_deref() != Some(session) {
                    continue;
                }
                if self.backend.get_story(id).map_err(text)?.status == "in_progress" {
                    return Err("session already claims another story".into());
                }
                release_claims.push(id.clone());
            }
        }
        let execution = ExternalExecution {
            sub_state: matches!(after.status, StoryStatus::Review | StoryStatus::Qa)
                .then_some(after.status),
            claim_session: after.claim_session.clone(),
            receipt_version: expected.receipt_version.clone(),
        };
        let mut written = raw;
        for op in &ops {
            written = self
                .backend
                .apply(&written.id, &written.version, op)
                .map_err(text)?;
        }
        let execution = ExternalExecution {
            receipt_version: if after.status == StoryStatus::Done {
                Some(written.version.clone())
            } else {
                execution.receipt_version
            },
            ..execution
        };
        ledger
            .store
            .record_external_transition(
                &ledger.adapter_id,
                &written.id,
                &expected,
                &execution,
                &ExternalTransition {
                    version: written.version.clone(),
                    command,
                    actor,
                },
                &release_claims,
            )
            .map_err(|error| format!("{error} (the adapter already holds the change)"))?;
        let ledger = self.ledger()?;
        Ok(self
            .overlay_all(vec![written], &ledger)?
            .pop()
            .expect("one story in, one out"))
    }

    fn evaluate(&self, current: &Current, command: &StoryCommand) -> Result<Story, String> {
        if matches!(
            command,
            StoryCommand::UncheckCriterion(_)
                | StoryCommand::SetCriterionManual { .. }
                | StoryCommand::WontFix
        ) {
            return Err(unsupported(&format!("{command:?}")));
        }
        let mut after = current.story.clone();
        let known = HashMap::new();
        evaluate_story_command(&mut after, command.clone(), &mut |story| {
            self.dependencies_met(&story.dependencies, &known, &current.ledger)
        })?;
        Ok(after)
    }

    pub(crate) fn transition(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        self.transition_unguarded(story_id, expected_revision, command, actor)
    }

    pub(crate) fn transition_unguarded(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        let current = self.read_current(story_id, expected_revision)?;
        current
            .ledger
            .store
            .check_transition_actor(&command, &actor)?;
        let after = self.evaluate(&current, &command)?;
        self.commit(current, after, command, actor)
    }

    /// Ready -> in progress (QA for manual-only work) with a session claim.
    pub(crate) fn claim(
        &self,
        story_id: &str,
        session: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        let current = self.read_current(story_id, expected_revision)?;
        if current.story.status != StoryStatus::Ready {
            return Err(if current.raw.status == "ready" {
                "story dependency lacks a current integration receipt".into()
            } else {
                format!(
                    "story is not ready (status: {}); only a ready story can be claimed",
                    current.raw.status
                )
            });
        }
        let mut after = current.story.clone();
        after.status = if only_manual_pending(&after) {
            StoryStatus::Qa
        } else {
            StoryStatus::InProgress
        };
        after.claim_session = (after.status != StoryStatus::Qa).then(|| session.into());
        let actor = StoryTransitionActor::ManagedSession {
            session_id: session.into(),
        };
        self.commit(current, after, StoryCommand::StartManual, actor)
    }

    pub(crate) fn available_transitions(
        &self,
        story_id: &str,
        expected_revision: i64,
        actor: StoryTransitionActor,
    ) -> Result<AvailableTransitions, String> {
        let current = self.read_current(story_id, expected_revision)?;
        let mut moves: Vec<StoryPhaseMove> = Vec::new();
        for command in [
            StoryCommand::StartManual,
            StoryCommand::SubmitReview,
            StoryCommand::SubmitQa,
            StoryCommand::Approve,
            StoryCommand::RejectReview,
            StoryCommand::RejectQa,
            StoryCommand::Block,
            StoryCommand::Unblock,
        ] {
            if current
                .ledger
                .store
                .check_transition_actor(&command, &actor)
                .is_err()
            {
                continue;
            }
            if let Ok(after) = self.evaluate(&current, &command)
                && after.status != current.story.status
                && !moves.iter().any(|item| item.status == after.status)
            {
                moves.push(StoryPhaseMove {
                    status: after.status,
                    command,
                });
            }
        }
        Ok(AvailableTransitions {
            revision: current.story.revision,
            moves,
        })
    }

    pub(crate) fn transition_history(
        &self,
        story_id: &str,
    ) -> Result<Vec<StoryTransition>, String> {
        let ledger = self.ledger()?;
        Ok(ledger
            .store
            .external_transition_history(&ledger.adapter_id, story_id)?
            .into_iter()
            .map(|entry| StoryTransition {
                story_id: story_id.into(),
                revision: revision(&entry.version),
                command: entry.command,
                actor: entry.actor,
            })
            .collect())
    }
}
