use super::{RunCommand, RunEvent, RunLimits, RunReceipt, RunSnapshot, RunStore};
use crate::workflows::{AgentRole, NodeKind, WorkflowStore};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use tauri::Emitter;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunAction {
    /// Start a pinned executable root through the owning daemon, never the legacy ledger.
    StartGraph {
        target: super::RunTarget,
        expected_revision: Option<i64>,
        definition_id: String,
        definition_revision: i64,
        request_id: String,
        #[serde(default)]
        limits: RunLimits,
    },
    StartPlan {
        plan_id: String,
        definition_id: String,
        definition_revision: i64,
        limits: RunLimits,
    },
    Incidents {
        run_id: String,
    },
    /// Say before a start what would stop the first Agent step.
    StartReadiness {
        definition_id: String,
        definition_revision: i64,
    },
    Get {
        run_id: String,
    },
    ListPlanRuns {
        plan_id: String,
        limit: usize,
    },
    Events {
        run_id: String,
        after_sequence: i64,
        limit: usize,
    },
    Command {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
        command: Box<RunCommand>,
    },
    RecordIntegration {
        run_id: String,
        story_id: String,
        command_id: String,
        expected_sequence: i64,
    },
    RecertifyCanonical {
        run_id: String,
        command_id: String,
        expected_sequence: i64,
    },
    ExecuteCheck {
        run_id: String,
        story_id: String,
        check_id: String,
        command_id: String,
        expected_sequence: i64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RunReply {
    Snapshot(Box<RunSnapshot>),
    Runs(Vec<RunSnapshot>),
    Events(Vec<RunEvent>),
    Receipt(Box<RunReceipt>),
    Incidents(Vec<super::RunIncident>),
    Readiness(Vec<String>),
}

/// The live coordinator is the only inbox recipient for story-worker results.
/// A plan may contain other agent roles, so a plan attempt alone is insufficient.
pub fn active_coordinator_session(run: &RunSnapshot) -> Result<Option<String>, String> {
    if !run.attempts.iter().any(|attempt| {
        attempt.story_id == run.plan_id && attempt.state == super::AttemptState::Running
    }) {
        return Ok(None);
    }
    let definition =
        WorkflowStore::open()?.get_published(&run.definition_id, run.definition_revision)?;
    let coordinator_nodes: std::collections::HashSet<&str> = definition
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            matches!(
                node.kind,
                NodeKind::Agent {
                    role: AgentRole::Coordinator,
                    ..
                }
            )
            .then_some(node.id.as_str())
        })
        .collect();
    let mut sessions = run.attempts.iter().filter_map(|attempt| {
        (attempt.story_id == run.plan_id
            && attempt.state == super::AttemptState::Running
            && coordinator_nodes.contains(attempt.node_id.as_str()))
        .then(|| attempt.agent.as_ref().map(|agent| agent.session_id.clone()))
        .flatten()
    });
    let session = sessions.next();
    if sessions.next().is_some() {
        return Err("more than one coordinator is active".into());
    }
    Ok(session)
}

pub fn run_action(project: &str, action: RunAction) -> Result<RunReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let owner = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = RunStore::open()?;
    match action {
        RunAction::StartGraph { .. } => {
            Err("graph start requires the owning daemon service".into())
        }
        RunAction::StartReadiness {
            definition_id,
            definition_revision,
        } => {
            let definition =
                WorkflowStore::open()?.get_published(&definition_id, definition_revision)?;
            Ok(RunReply::Readiness(
                super::runtime::effects::profile_problems(
                    &definition,
                    &crate::config::load_agents_config(),
                ),
            ))
        }
        RunAction::StartPlan {
            plan_id,
            definition_id,
            definition_revision,
            limits,
        } => Ok(RunReply::Snapshot(Box::new(store.start_plan(
            &owner,
            &plan_id,
            &definition_id,
            definition_revision,
            limits,
        )?))),
        RunAction::Incidents { run_id } => {
            let run = scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Incidents(super::incidents::read_incidents(
                &run, None,
            )?))
        }
        RunAction::Get { run_id } => Ok(RunReply::Snapshot(Box::new(scoped_snapshot(
            &store, &owner, &run_id,
        )?))),
        RunAction::ListPlanRuns { plan_id, limit } => {
            let plan = crate::stories::StoryStore::open()?.get_plan(&plan_id)?;
            if plan.project != owner {
                return Err("plan does not belong to project".into());
            }
            Ok(RunReply::Runs(
                store.list_plan_runs(&owner, &plan_id, limit)?,
            ))
        }
        RunAction::Events {
            run_id,
            after_sequence,
            limit,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Events(store.events_after(
                &run_id,
                after_sequence,
                limit,
            )?))
        }
        RunAction::Command {
            run_id,
            command_id,
            expected_sequence,
            command,
        } => {
            let command = *command;
            scoped_snapshot(&store, &owner, &run_id)?;
            if matches!(
                command,
                RunCommand::ExpireDeadline
                    | RunCommand::StartGraphAgent { .. }
                    | RunCommand::Graph { .. }
                    | RunCommand::BindAgent { .. }
                    | RunCommand::ReportBoundAttempt { .. }
                    | RunCommand::AssignWorktree { .. }
                    | RunCommand::RecordCheck { .. }
                    | RunCommand::RecordIntegration { .. }
                    | RunCommand::RecordRecertification { .. }
            ) {
                return Err(
                    "graph transitions and verified receipts require their internal backend service; agent binding, reports, and worktree assignment require a managed MCP session"
                        .into(),
                );
            }
            Ok(RunReply::Receipt(Box::new(store.command_expected(
                &run_id,
                &command_id,
                expected_sequence,
                command,
            )?)))
        }
        RunAction::RecordIntegration {
            run_id,
            story_id,
            command_id,
            expected_sequence,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Receipt(Box::new(store.record_integrated_story(
                &run_id,
                &story_id,
                &command_id,
                expected_sequence,
            )?)))
        }
        RunAction::RecertifyCanonical {
            run_id,
            command_id,
            expected_sequence,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Receipt(Box::new(store.recertify_canonical(
                &run_id,
                &command_id,
                expected_sequence,
            )?)))
        }
        RunAction::ExecuteCheck {
            run_id,
            story_id,
            check_id,
            command_id,
            expected_sequence,
        } => {
            scoped_snapshot(&store, &owner, &run_id)?;
            Ok(RunReply::Receipt(Box::new(store.execute_check(
                &run_id,
                &story_id,
                &check_id,
                &command_id,
                expected_sequence,
            )?)))
        }
    }
}

/// Send a wake hint after a committed mutation. The event stream remains the
/// source of truth when a client misses or receives duplicate hints.
pub fn run_action_with_events(
    state: &crate::state::AppState,
    project: &str,
    action: RunAction,
) -> Result<RunReply, String> {
    if let RunAction::Incidents { run_id } = &action {
        let RunReply::Snapshot(run) = run_action(
            project,
            RunAction::Get {
                run_id: run_id.clone(),
            },
        )?
        else {
            unreachable!("get returns a snapshot")
        };
        return Ok(RunReply::Incidents(super::incidents::read_incidents(
            &run,
            Some(state),
        )?));
    }
    let mutation = matches!(
        action,
        RunAction::StartGraph { .. }
            | RunAction::StartPlan { .. }
            | RunAction::Command { .. }
            | RunAction::RecordIntegration { .. }
            | RunAction::RecertifyCanonical { .. }
            | RunAction::ExecuteCheck { .. }
    );
    if mutation {
        state.workflow_runtime.require_owner()?;
    }
    let graph_start = matches!(action, RunAction::StartGraph { .. });
    let reply = if let RunAction::StartGraph {
        target,
        expected_revision,
        definition_id,
        definition_revision,
        request_id,
        limits,
    } = action
    {
        if !crate::fs::is_absolute_on_any_platform(project) {
            return Err("project must be an absolute path".into());
        }
        RunReply::Snapshot(Box::new(state.workflow_runtime.start_graph(
            state,
            &super::store::GraphStartRequest {
                project: project.into(),
                target,
                expected_revision,
                definition_id,
                definition_revision,
                request_id,
                limits,
            },
        )?))
    } else {
        run_action(project, action)?
    };
    if mutation {
        let (repo_path, run_id, sequence) = match &reply {
            RunReply::Snapshot(snapshot) => (&snapshot.project, &snapshot.id, snapshot.sequence),
            RunReply::Receipt(receipt) => (
                &receipt.snapshot.project,
                &receipt.snapshot.id,
                receipt.sequence,
            ),
            RunReply::Events(_)
            | RunReply::Runs(_)
            | RunReply::Incidents(_)
            | RunReply::Readiness(_) => {
                unreachable!("mutations return a snapshot or receipt")
            }
        };
        if !graph_start {
            emit_run_changed(state, repo_path, run_id, sequence);
        }
    }
    Ok(reply)
}

pub fn emit_run_changed(
    state: &crate::state::AppState,
    repo_path: &str,
    run_id: &str,
    sequence: i64,
) {
    emit_run_cursor(state, repo_path, run_id, sequence);
    state.workflow_runtime.wake(run_id);
}

pub(super) fn emit_run_cursor(
    state: &crate::state::AppState,
    repo_path: &str,
    run_id: &str,
    sequence: i64,
) {
    let payload = serde_json::json!({ "runId": run_id, "sequence": sequence });
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(
            "workflow-run-changed",
            serde_json::json!({
                "repo_path": repo_path, "payload": &payload,
            }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::WorkflowRunChanged {
            repo_path: repo_path.to_owned(),
            payload,
        });
}

fn scoped_snapshot(store: &RunStore, owner: &str, run_id: &str) -> Result<RunSnapshot, String> {
    let snapshot = store.snapshot(run_id)?;
    if snapshot.project != owner {
        return Err("workflow run does not belong to project".into());
    }
    Ok(snapshot)
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn run_decisions_reject_relative_projects() {
        // Catches: run decisions accepting a relative project path.
        for command in [
            RunCommand::AnswerInput {
                attempt_id: "attempt".into(),
                answer: "yes".into(),
            },
            RunCommand::Resume,
            RunCommand::FinalVerificationPassed,
            RunCommand::Complete,
            RunCommand::ResolveUncertainEffect {
                effect_id: "effect".into(),
                succeeded: true,
            },
        ] {
            let error = run_action(
                "relative",
                RunAction::Command {
                    run_id: "run".into(),
                    command_id: "decision".into(),
                    expected_sequence: 1,
                    command: Box::new(command),
                },
            )
            .unwrap_err();
            assert_eq!(error, "project must be an absolute path");
        }
    }
}
