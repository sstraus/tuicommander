//! Source generations come from retained trigger admission, never workflow labels.
use super::{GitHubAction, GitHubRequest};
use crate::workflows::run::{RunStore, neutral_trigger_store::TriggerIntent};
use crate::workflows::{github_trigger::GithubResource, trigger::TriggerSource};
use rusqlite::OptionalExtension;

pub(super) fn current(
    store: &RunStore,
    run: &str,
    request: &GitHubRequest,
) -> Result<Option<TriggerIntent>, String> {
    let request_id: String = store
        .trigger_connection()?
        .query_row(
            "SELECT request_id FROM neutral_workflow_runs WHERE id=?1",
            [run],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let Some(receipt) = request_id.strip_prefix("trigger:") else {
        return Ok(None);
    };
    let intent = store
        .trigger_intent(receipt)?
        .ok_or("publication source intent missing")?;
    if intent.event.source == TriggerSource::Github && intent.event.kind == "issue" {
        return issue_source(store, run, request, intent).map(Some);
    }
    let number = match &request.action {
        GitHubAction::Review { number, .. } | GitHubAction::MarkReady { number } => *number,
        _ => return Ok(None),
    };
    if intent.event.source != TriggerSource::Github
        || intent.event.kind != "pull_request"
        || intent.event.resource
            != format!(
                "github:{}/pull_request/{number}",
                request.repository.to_ascii_lowercase()
            )
        || intent.event.values.get("head") != Some(&serde_json::json!(request.commit))
    {
        return Err("publication differs from retained PR source".into());
    }
    if let GitHubAction::Review { base, .. } = &request.action
        && intent.event.values.get("base") != Some(&serde_json::json!(base))
    {
        return Err("publication base differs from retained PR source".into());
    }
    let current = store.retained_github_events(
        &request.repository.to_ascii_lowercase(),
        GithubResource::PullRequest,
        false,
    )?;
    if !current.iter().any(|event| {
        event.resource == intent.event.resource && event.generation == intent.event.generation
    }) {
        return Err("publication source generation is superseded or unavailable".into());
    }
    // The resource row fences this run against replacement admission as well.
    let active: Option<String> = store.trigger_connection()?.query_row(
        "SELECT active_run FROM neutral_trigger_resources WHERE project=?1 AND binding_id=?2 AND resource=?3",
        rusqlite::params![intent.project, intent.admission.binding_id, intent.event.resource], |row| row.get(0),
    ).optional().map_err(|e| e.to_string())?.flatten();
    if active.as_deref() != Some(run) {
        return Err("publication no longer owns the source resource".into());
    }
    Ok(Some(intent))
}

/// An issue admission is not blanket PR authority. Ready requires this run's
/// exact stopped draft-PR publication, including provider number and generation.
fn issue_source(
    store: &RunStore,
    run: &str,
    request: &GitHubRequest,
    intent: TriggerIntent,
) -> Result<TriggerIntent, String> {
    if !matches!(
        request.action,
        GitHubAction::PushBranch { .. }
            | GitHubAction::OpenPr { .. }
            | GitHubAction::MarkReady { .. }
    ) {
        return Err("issue admission does not authorize this publication action".into());
    }
    let number = intent
        .event
        .values
        .get("number")
        .and_then(serde_json::Value::as_i64)
        .ok_or("issue source number absent")?;
    if number <= 0
        || intent.event.resource
            != format!(
                "github:{}/issue/{number}",
                request.repository.to_ascii_lowercase()
            )
        || intent.admission.run_id.as_deref() != Some(run)
    {
        return Err("publication differs from retained issue source".into());
    }
    let current = store.retained_github_events(
        &request.repository.to_ascii_lowercase(),
        GithubResource::Issue,
        false,
    )?;
    if !current.iter().any(|event| {
        event.resource == intent.event.resource && event.generation == intent.event.generation
    }) {
        return Err("publication issue source generation is superseded or unavailable".into());
    }
    let active: Option<String> = store.trigger_connection()?.query_row(
        "SELECT active_run FROM neutral_trigger_resources WHERE project=?1 AND binding_id=?2 AND resource=?3",
        rusqlite::params![intent.project, intent.admission.binding_id, intent.event.resource], |r| r.get(0),
    ).optional().map_err(|e| e.to_string())?.flatten();
    if active.as_deref() != Some(run) {
        return Err("publication no longer owns the issue resource".into());
    }
    if matches!(request.action, GitHubAction::MarkReady { .. }) {
        issue_draft(store, run, request, &intent.event.generation)?;
    }
    Ok(intent)
}

fn issue_draft(
    store: &RunStore,
    run: &str,
    request: &GitHubRequest,
    generation: &str,
) -> Result<GitHubRequest, String> {
    let GitHubAction::MarkReady { number } = request.action else {
        return Err("expected mark-ready".into());
    };
    use crate::workflows::run::{neutral_effects::EffectOutcome, neutral_execution::NeutralResult};
    let snapshot = store.neutral_execution(run)?;
    let mut matches = Vec::new();
    for effect in snapshot.effects.values() {
        let issue_wrapper = effect.invocation.descriptor.handler.name
            == super::super::native_issue_publication::HANDLER;
        if (!issue_wrapper
            && effect.invocation.descriptor.handler.name != "native.publication.github")
            || !effect.settled()
        {
            continue;
        }
        let original: GitHubRequest = serde_json::from_value(if issue_wrapper {
            effect.invocation.inputs["request"]["publication"].clone()
        } else {
            effect.invocation.inputs["request"].clone()
        })
        .map_err(|e| e.to_string())?;
        if !matches!(original.action, GitHubAction::OpenPr { .. })
            || original.account != request.account
            || original.repository != request.repository
            || original.commit != request.commit
            || original.tree != request.tree
        {
            continue;
        }
        let Some(super::super::neutral_effects::EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
            stopped: true,
        }) = &effect.observation
        else {
            continue;
        };
        let value = outputs.get("receipt").ok_or("draft PR receipt absent")?;
        let receipt: super::GitHubReceipt = serde_json::from_value(if issue_wrapper {
            value["publication"].clone()
        } else {
            value.clone()
        })
        .map_err(|e| e.to_string())?;
        if receipt.account == request.account
            && receipt.repository == request.repository
            && receipt.commit == request.commit
            && receipt.number == Some(number)
            && receipt.source_generation.as_deref() == Some(generation)
            && serde_json::to_value(&receipt.action).map_err(|e| e.to_string())?
                == serde_json::to_value(&original.action).map_err(|e| e.to_string())?
        {
            matches.push(original);
        }
    }
    if matches.len() != 1 {
        return Err("issue mark-ready requires this run's exact stopped draft PR receipt".into());
    }
    Ok(matches.remove(0))
}

/// The live PR must still target the exact base selected by the owned creation.
pub(super) fn require_issue_ready_base(
    store: &RunStore,
    run: &str,
    request: &GitHubRequest,
    source: Option<&TriggerIntent>,
    observed: &serde_json::Value,
) -> Result<(), String> {
    let Some(source) = source.filter(|source| source.event.kind == "issue") else {
        return Ok(());
    };
    let original = issue_draft(store, run, request, &source.event.generation)?;
    let GitHubAction::OpenPr { base, .. } = original.action else {
        return Err("owned draft action missing".into());
    };
    if observed
        .pointer("/base/ref")
        .and_then(serde_json::Value::as_str)
        != Some(base.as_str())
    {
        return Err("issue PR base differs from exact draft publication".into());
    }
    Ok(())
}
