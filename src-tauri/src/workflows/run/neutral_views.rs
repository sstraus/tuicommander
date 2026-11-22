//! Read-only presentation derived from verified ledger replay, never current draft graphs.
use super::neutral::{NeutralEvent, NeutralEventKind};
use super::neutral_effects::{EffectDecision, NeutralEffect};
use super::neutral_execution::{NeutralDecision, NeutralExecution, NeutralStatus, replay_observed};
use super::store::RunStore;
use crate::workflows::WorkflowStore;
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn display_name(project: &str, id: &str) -> Result<String, String> {
    WorkflowStore::open()?
        .connect()?
        .query_row(
            "SELECT name FROM neutral_workflow_drafts WHERE project=?1 AND id=?2",
            rusqlite::params![project, id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map(|name| name.unwrap_or_else(|| id.into()))
        .map_err(|e| e.to_string())
}

pub(super) fn pending(effect: &NeutralEffect, snapshot: &NeutralExecution, now: i64) -> bool {
    snapshot.status == NeutralStatus::Running
        && snapshot.deadline_ms > now
        && !effect.cancel_started
        && effect.answer.is_none()
        && effect.observation.is_none()
        && effect
            .question
            .as_ref()
            .is_some_and(|q| q.deadline_ms > now)
}

impl RunStore {
    pub(crate) fn neutral_inspection_view(
        &self,
        id: &str,
        after: i64,
        limit: usize,
    ) -> Result<Value, String> {
        if after < 0 || limit == 0 || limit > 500 {
            return Err(crate::workflows::neutral_api::refusal::INVALID_CURSOR.into());
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let mut states: BTreeMap<String, BTreeMap<String, &str>> = BTreeMap::new();
        let (snapshot, events) = replay_observed(&tx, id, None, |current, decision| {
            let nodes = states.entry(current.execution.clone()).or_default();
            match decision {
                NeutralDecision::Route { .. } | NeutralDecision::Finish { .. } => {
                    nodes.insert(current.node.clone(), "completed");
                }
                NeutralDecision::Failure { .. } => {
                    nodes.insert(current.node.clone(), "failed");
                }
                _ => {}
            }
            Ok(())
        })?;
        let now = super::store::now_ms();
        let approvals = snapshot.effects.values().filter(|e| pending(e, &snapshot, now))
            .map(|effect| json!({"key":effect.key,"question":effect.question,
                "operator_request": (effect.invocation.descriptor.handler.name == super::native_operator::HANDLER)
                    .then(|| effect.invocation.inputs.get("request")).flatten()}))
            .collect::<Vec<_>>();
        let resource_waits: Vec<_> = if snapshot.status == NeutralStatus::Running {
            snapshot
                .effects
                .values()
                .filter(|e| !e.cancel_started && e.observation.is_none())
                .filter_map(|e| e.resource_wait.as_ref())
                .collect()
        } else {
            Vec::new()
        };
        let waiting = !approvals.is_empty();
        let nodes = states.entry(snapshot.execution.clone()).or_default();
        nodes.insert(
            snapshot.node.clone(),
            match snapshot.status {
                NeutralStatus::Running if waiting => "waiting",
                NeutralStatus::Running if snapshot.wait_due_ms.is_some() => "timer_wait",
                NeutralStatus::Running if !resource_waits.is_empty() => "resource_wait",
                NeutralStatus::Running => "current",
                NeutralStatus::Failed { .. } => "failed",
                NeutralStatus::Completed { .. } => "completed",
                _ => "stopped",
            },
        );
        let started = events.first().map(|e| e.at_ms);
        let updated = events.last().map(|e| e.at_ms);
        let progress = json!({"node_states": nodes, "waiting_on_approval": !approvals.is_empty(),
            "name": display_name(&snapshot.run.request.publication.project, &snapshot.run.request.publication.id)?,
            "server_now_ms":now,"approvals":approvals,"resource_waits":resource_waits,"started_ms": started,"updated_ms":updated,
            "publication": WorkflowStore::open()?.neutral_publication_view(snapshot.publication.clone())?});
        let events = events
            .into_iter()
            .filter(|e| e.sequence > after)
            .take(limit)
            .collect::<Vec<_>>();
        tx.commit().map_err(|e| e.to_string())?;
        Ok(json!({"snapshot":snapshot,"events":events,"progress":progress}))
    }
}

/// Question creation is a durable event; ordinary progress never repeats its push.
pub(super) fn new_question(event: &NeutralEvent) -> Option<&str> {
    match &event.kind {
        NeutralEventKind::Execution { decision, .. } => match decision.as_ref() {
            NeutralDecision::Effect { decision } => match decision.as_ref() {
                EffectDecision::Prepare { effect } => {
                    effect.question.as_ref().map(|q| q.title.as_str())
                }
                EffectDecision::Question { question, .. } => Some(&question.title),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
