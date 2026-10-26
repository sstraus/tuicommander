//! Atomic graph-root start, using the same events and projections as commands.
use super::*;
use crate::workflows::run::graph::{GraphEvent, GraphExecution, GraphTransition};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GraphStartRequest {
    pub project: String,
    pub target: RunTarget,
    pub expected_revision: Option<i64>,
    pub definition_id: String,
    pub definition_revision: i64,
    pub request_id: String,
    pub limits: RunLimits,
}

impl RunStore {
    /// Commit the root and its first successor together, before any daemon wake.
    pub(crate) fn start_graph_run(
        &self,
        request: &GraphStartRequest,
    ) -> Result<RunSnapshot, RunStartError> {
        validate_key("start request id", &request.request_id)?;
        request.limits.validate()?;
        let owner = crate::progress::resolve_owning_project(Some(&request.project))?
            .to_string_lossy()
            .to_string();
        let mut canonical_request = request.clone();
        canonical_request.project = owner.clone();
        let hash = hex::encode(Sha256::digest(encode(&canonical_request)?.as_bytes()));
        let key = format!("start:{}", request.request_id);
        // A retry remains valid after edits, cancellation, or daemon restart.
        if let Some(snapshot) = start_retry(&self.connect()?, &owner, &key, &hash)? {
            return Ok(snapshot);
        }
        let stories = StoryStore::open()?;
        let (plan_id, target_id, kind, revision) = match &request.target {
            RunTarget::Plan(id) => {
                let plan = stories.require_unarchived_plan(id)?;
                if plan.project != owner {
                    return Err("plan does not belong to project".into());
                }
                (id.clone(), id.clone(), WorkflowKind::Plan, None)
            }
            RunTarget::Story(id) => {
                let story = stories.require_unarchived_story(id)?;
                let plan = stories.get_plan(&story.plan_id)?;
                if plan.project != owner {
                    return Err("story does not belong to project".into());
                }
                if story.claim_session.is_some() {
                    return Err("story has a live manual claim".into());
                }
                if story.status != StoryStatus::Ready {
                    return Err("only a ready story can start a workflow".into());
                }
                (
                    story.plan_id,
                    id.clone(),
                    WorkflowKind::Story,
                    Some(story.revision),
                )
            }
        };
        if revision != request.expected_revision {
            return Err("stale workflow target revision".into());
        }
        let definitions = WorkflowStore::open()?;
        let definition =
            definitions.get_published(&request.definition_id, request.definition_revision)?;
        if definition.project != owner || definition.kind != kind {
            return Err("published workflow does not match the root target".into());
        }
        let (story_definition_id, story_definition_revision) = if kind == WorkflowKind::Story {
            (definition.id.clone(), definition.revision)
        } else {
            let dispatches: Vec<_> = definition
                .graph
                .nodes
                .iter()
                .filter_map(|node| {
                    if let NodeKind::StoryDispatch {
                        story_template_id,
                        story_revision,
                    } = &node.kind
                    {
                        Some((story_template_id.clone(), *story_revision))
                    } else {
                        None
                    }
                })
                .collect();
            if dispatches.len() != 1 {
                return Err("plan workflow requires one Story Dispatch node".into());
            }
            let (id, revision) = dispatches[0].clone();
            let story = definitions.get_published(&id, revision)?;
            if story.kind != WorkflowKind::Story || story.project != owner {
                return Err("pinned story workflow does not belong to project".into());
            }
            (id, revision)
        };
        // DEFERRED (2026-10-05) — dependency preflight: graph start skips the preflight that
        // manual start runs (`validate_preflight`); needed before Agent effects in slice C.
        let execution =
            GraphExecution::start("root".into(), target_id.clone(), definition.clone())?;
        let initial = RunSnapshot {
            event_contract_version: super::super::graph::RUN_EVENT_CONTRACT_VERSION,
            id: Uuid::now_v7().to_string(),
            canonical_ref: git_output(Path::new(&owner), &["symbolic-ref", "HEAD"]).ok(),
            project: owner.clone(),
            plan_id,
            root_target: Some(request.target.clone()),
            definition_id: definition.id,
            definition_revision: definition.revision,
            story_definition_id,
            story_definition_revision,
            status: RunStatus::Running,
            sequence: 0,
            started_ms: now_ms(),
            paused_since_ms: None,
            paused_duration_ms: 0,
            pause_cause: None,
            limits: request.limits.clone(),
            loops: 0,
            story_creations: 0,
            spawns: 0,
            planning_fingerprint: None,
            verification_fingerprint: None,
            stories: vec![],
            canonical_recertification: None,
            attempts: vec![],
            effects: vec![],
            graph_executions: vec![],
        };
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin graph root start: {e}"))?;
        if let Some(snapshot) = start_retry(&tx, &owner, &key, &hash)? {
            return Ok(snapshot);
        }
        if kind == WorkflowKind::Story {
            require_available_story(&tx, &owner, &target_id)?;
        }
        // Recheck native identity after obtaining the run writer lock.
        let current_revision = match &request.target {
            RunTarget::Plan(id) => {
                stories.require_unarchived_plan(id)?;
                None
            }
            RunTarget::Story(id) => {
                let story = stories.require_unarchived_story(id)?;
                if story.claim_session.is_some() {
                    return Err("story has a live manual claim".into());
                }
                if story.status != StoryStatus::Ready {
                    return Err("only a ready story can start a workflow".into());
                }
                Some(story.revision)
            }
        };
        if current_revision != revision {
            return Err("workflow target changed during start; retry".into());
        }
        let event = RunEvent {
            sequence: 1,
            command_id: key,
            command_hash: Some(hash),
            at_ms: initial.started_ms,
            kind: RunEventKind::Started {
                initial: Box::new(initial),
            },
        };
        let snapshot = apply_event(None, &event)?;
        insert_started_run(&tx, &snapshot, "start graph run")?;
        insert_event(
            &tx,
            &snapshot.id,
            &RunReceipt {
                sequence: 1,
                event,
                snapshot: snapshot.clone(),
            },
        )?;
        let at = snapshot.started_ms;
        let receipt = persist_event(
            &tx,
            snapshot,
            "initial:graph",
            None,
            at,
            RunEventKind::Graph {
                event: GraphEvent::Started {
                    execution: Box::new(execution),
                },
            },
        )?;
        let receipt = persist_event(
            &tx,
            receipt.snapshot,
            "initial:activate",
            None,
            at,
            RunEventKind::Graph {
                event: GraphEvent::Transition {
                    transition: GraphTransition::Activate {
                        execution_id: "root".into(),
                        activation_id: "a0".into(),
                    },
                },
            },
        )?;
        let receipt = persist_event(
            &tx,
            receipt.snapshot,
            "initial:successor",
            None,
            at,
            RunEventKind::Graph {
                event: GraphEvent::Transition {
                    transition: GraphTransition::Complete {
                        execution_id: "root".into(),
                        activation_id: "a0".into(),
                        outcome: None,
                        evidence: None,
                    },
                },
            },
        )?;
        tx.commit()
            .map_err(|e| format!("commit graph root start: {e}"))?;
        Ok(receipt.snapshot)
    }
}

fn start_retry(
    conn: &Connection,
    project: &str,
    key: &str,
    hash: &str,
) -> Result<Option<RunSnapshot>, String> {
    let row: Option<(String, String)> = conn.query_row(
        "SELECT r.snapshot_json,e.event_json FROM workflow_events e JOIN workflow_runs r ON r.id=e.run_id WHERE r.project=?1 AND e.command_id=?2 AND e.sequence=1",
        params![project, key], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional().map_err(|e| format!("read workflow start retry: {e}"))?;
    row.map(|(snapshot, event)| {
        if decode::<RunEvent>(&event)?.command_hash.as_deref() != Some(hash) {
            return Err("workflow start request id was reused with a different payload".into());
        }
        decode(&snapshot)
    })
    .transpose()
}

/// Active graph projections are the durable reservation; cancellation releases them.
pub(super) fn require_available_story(
    conn: &Connection,
    project: &str,
    story_id: &str,
) -> Result<(), String> {
    let mut stmt = conn.prepare("SELECT snapshot_json FROM workflow_runs WHERE project=?1 AND status IN ('running','paused')")
        .map_err(|e| format!("prepare workflow reservations: {e}"))?;
    for row in stmt
        .query_map([project], |row| row.get::<_, String>(0))
        .map_err(|e| format!("read workflow reservations: {e}"))?
    {
        let run: RunSnapshot = decode(&row.map_err(|e| format!("read reserved workflow: {e}"))?)?;
        if run
            .graph_executions
            .iter()
            .any(|graph| graph.target_id == story_id)
        {
            return Err("story is reserved by another workflow root".into());
        }
    }
    Ok(())
}
