//! Read-only incident projection. Never infer failure from elapsed wall time.
use super::{AttemptOutcome, AttemptState, NodeAttempt, RunSnapshot, RunStatus};
use crate::state::AppState;
use crate::tasks::TaskStatus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunIncident {
    pub run_id: String,
    pub attempt_id: Option<String>,
    pub story_id: Option<String>,
    pub node_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub source: String,
    pub cause: String,
    pub next_action: String,
}

fn incident(
    run: &RunSnapshot,
    attempt: Option<&NodeAttempt>,
    source: &str,
    cause: String,
    next_action: &str,
) -> RunIncident {
    let agent = attempt.and_then(|attempt| attempt.agent.as_ref());
    RunIncident {
        run_id: run.id.clone(),
        attempt_id: attempt.map(|attempt| attempt.id.clone()),
        story_id: attempt.map(|attempt| attempt.story_id.clone()),
        node_id: attempt.map(|attempt| attempt.node_id.clone()),
        session_id: agent.map(|agent| agent.session_id.clone()),
        task_id: agent.and_then(|agent| agent.task_id.clone()),
        source: source.into(),
        cause,
        next_action: next_action.into(),
    }
}

pub(super) fn read_incidents(
    run: &RunSnapshot,
    state: Option<&AppState>,
) -> Result<Vec<RunIncident>, String> {
    let stories = crate::stories::StoryStore::open()?.list_stories(&run.plan_id)?;
    Ok(project_story_incidents(run, state, &stories))
}

fn project_story_incidents(
    run: &RunSnapshot,
    state: Option<&AppState>,
    stories: &[crate::stories::Story],
) -> Vec<RunIncident> {
    let mut incidents = project_incidents(run, state);
    for story in stories {
        if story.status != crate::stories::StoryStatus::Qa
            || matches!(&run.root_target, Some(super::RunTarget::Story(id)) if id != &story.id)
        {
            continue;
        }
        let attempt = run
            .attempts
            .iter()
            .rev()
            .find(|attempt| attempt.story_id == story.id);
        let mut item = incident(
            run,
            attempt,
            "story_qa",
            format!("Story waits for manual QA: {}", story.title),
            "Perform the manual checks, check each passing criterion and approve; use Reject QA if a check fails.",
        );
        item.story_id = Some(story.id.clone());
        incidents.push(item);
    }
    incidents
}

pub(super) fn project_incidents(run: &RunSnapshot, state: Option<&AppState>) -> Vec<RunIncident> {
    let mut incidents = Vec::new();
    for attempt in &run.attempts {
        let report = attempt.report.as_ref();
        let summary = report
            .map(|report| report.summary.trim())
            .filter(|summary| !summary.is_empty());
        let recorded = match attempt.outcome {
            Some(AttemptOutcome::Failed) => Some((
                "workflow_report",
                summary.unwrap_or("Attempt reported failure").to_owned(),
                "Inspect the report and story evidence, then choose the next attempt manually.",
            )),
            Some(AttemptOutcome::Interrupted) => Some((
                "attempt_interrupted",
                summary
                    .unwrap_or("Attempt was interrupted; no completed outcome is recorded")
                    .to_owned(),
                "Inspect the previous attempt and its session before starting replacement work manually.",
            )),
            Some(AttemptOutcome::NeedsInput) if attempt.input_answer.is_none() => Some((
                "workflow_report",
                report
                    .and_then(|report| report.input_request.as_ref())
                    .map(|input| input.question.clone())
                    .unwrap_or_else(|| summary.unwrap_or("Attempt needs input").into()),
                "Record an answer in this run's input form; resume only when ready.",
            )),
            _ => None,
        };
        if let Some((source, cause, next)) = recorded {
            incidents.push(incident(run, Some(attempt), source, cause, next));
        }
        // Completed historical attempts must not inherit today's live session/task state.
        if attempt.state == AttemptState::Reported
            || attempt.outcome == Some(AttemptOutcome::Completed)
        {
            continue;
        }
        let Some((state, agent)) = state.zip(attempt.agent.as_ref()) else {
            continue;
        };
        if let Some(pending) = state.pending_initial_prompts.get(&agent.session_id)
            && pending.notified
        {
            incidents.push(incident(run, Some(attempt), "prompt_delivery_failed", "Initial prompt delivery timed out; the prompt is still queued".into(), "Inspect the session for a startup dialog. The existing queue may still deliver the prompt; check before sending it again."));
        }
        if let Some(code) = state.session_maps.exit_codes.get(&agent.session_id) {
            incidents.push(incident(run, Some(attempt), "session_exit", format!("Session exited with code {}; no workflow outcome is recorded", *code), "Inspect the session output and reconcile the attempt before starting replacement work manually."));
        } else if state
            .session_maps
            .session_states
            .get(&agent.session_id)
            .is_some_and(|session| session.agent_state.as_deref() == Some("awaiting_input"))
        {
            incidents.push(incident(
                run,
                Some(attempt),
                "state_change",
                "Session is awaiting input".into(),
                "Open the bound session and inspect its question or approval request.",
            ));
        }
        if let Some(task) = agent.task_id.as_deref().and_then(|id| state.tasks.get(id)) {
            // A task ID alone is insufficient: retain the explicit session binding.
            if task.session_id.as_deref() == Some(agent.session_id.as_str())
                && matches!(
                    task.status,
                    TaskStatus::Failed | TaskStatus::InputRequired | TaskStatus::Cancelled
                )
            {
                let cause = task
                    .error
                    .or(task.status_message)
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or_else(|| format!("Agent task is {}", task.status.as_str()));
                incidents.push(incident(run, Some(attempt), "task_record", cause, "Inspect the task record and bound session before choosing a manual recovery action."));
            }
        }
    }
    if incidents.is_empty() && run.status == RunStatus::Paused {
        incidents.push(match run.pause_cause.as_deref() {
            Some(cause) => incident(
                run,
                None,
                "pause_cause",
                cause.into(),
                "Fix the cause, then use Resume run on the pending activation.",
            ),
            None => incident(
                run,
                None,
                "run_state",
                "Run is paused; no more specific cause is retained".into(),
                "Inspect the run timeline and evidence; use Resume run only when ready.",
            ),
        });
    }
    incidents
}

#[cfg(test)]
mod tests;
