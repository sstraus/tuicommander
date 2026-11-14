//! Existing story cohorts are snapshots, not a scheduler or a completion claim.
use super::native_operations::NativeContext;
use super::neutral_effects::EffectKey;
use crate::stories::{Story, StoryBackend, StoryStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const HANDLER: &str = "host.story.subflow";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryPin {
    pub id: String,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cohort {
    pub plan: String,
    pub plan_revision: i64,
    pub source: String,
    /// Order is retained. This is not a combined-artifact integration barrier.
    pub stories: Vec<StoryPin>,
}

impl Cohort {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.plan.trim().is_empty()
            || self.plan_revision < 1
            || self.source.trim().is_empty()
            || self.stories.is_empty()
            || self.stories.len() > 128
            || self
                .stories
                .iter()
                .any(|s| s.id.trim().is_empty() || s.revision < 1)
            || self
                .stories
                .iter()
                .map(|s| &s.id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.stories.len()
        {
            return Err("story dispatch requires a nonempty ordered unique pinned cohort".into());
        }
        Ok(())
    }

    pub(super) fn current(
        &self,
        context: &NativeContext,
        backend: &dyn StoryBackend,
        entering: bool,
    ) -> Result<Vec<Story>, String> {
        self.validate()?;
        context.current(None)?;
        let plan = backend.require_unarchived_plan(&self.plan)?;
        if plan.revision != self.plan_revision
            || plan.source != self.source
            || plan.project != context.workspace_path().to_string_lossy()
        {
            return Err("story dispatch plan/source differs from inherited current scope".into());
        }
        self.stories
            .iter()
            .map(|pin| {
                let story = backend.require_unarchived_story(&pin.id)?;
                if story.plan_id != self.plan
                    || story.revision < pin.revision
                    || (entering && story.revision != pin.revision)
                {
                    return Err("story dispatch cohort revision/project is stale or foreign".into());
                }
                Ok(story)
            })
            .collect()
    }
}

pub(super) fn observe(
    context: &NativeContext,
    key: &EffectKey,
    backend: &dyn StoryBackend,
    cohort: &Cohort,
) -> Result<serde_json::Value, String> {
    let stories = cohort.current(context, backend, false)?;
    let completed = stories
        .iter()
        .all(|story| story.status == StoryStatus::Done);
    Ok(serde_json::json!({
        "owner": key,
        "cohort": cohort,
        "outcome": if completed { "completed" } else { "blocked" },
        "stories": stories,
        "integration_barrier": "not_attested",
        "integration_obligations": {
            "combined_artifact": "blocked",
            "independent_batch_review": "blocked",
            "fix_reentry": "blocked",
        },
    }))
}

pub(super) fn admit(
    context: &NativeContext,
    instance: &crate::workflows::catalog::ActivityInstance,
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
    subject: Option<&std::collections::BTreeMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let supplied = inputs
        .get("cohort")
        .ok_or("child dispatch cohort input missing")?;
    let cohort: Cohort = serde_json::from_value(supplied.clone()).map_err(|e| e.to_string())?;
    match instance.config.get("cohort") {
        Some(snapshot) if snapshot != supplied => {
            return Err("child dispatch inputs differ from the pinned cohort".into());
        }
        Some(_) => {}
        None => created_in_run(context, &cohort)?,
    }
    let workspace = subject
        .and_then(|s| s.get("workspace"))
        .ok_or("story dispatch workspace absent")?;
    if crate::workflows::data_reference::DataReference::parse(workspace)?
        != (crate::workflows::data_reference::DataReference::Workspace {
            id: context.workspace_path().to_string_lossy().into_owned(),
        })
    {
        return Err("story dispatch workspace exceeds inherited scope".into());
    }
    cohort
        .current(
            context,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
            true,
        )
        .map(|_| ())
}

/// A story this run created, from its settled native.story.create receipt.
pub(super) struct Created {
    pub id: String,
    pub revision: i64,
    pub plan: String,
    pub review: String,
}

/// This run's created stories in creation order (activation, then attempt).
pub(super) fn created_stories(
    snapshot: &super::neutral_execution::NeutralExecution,
) -> Vec<Created> {
    let mut created: Vec<_> = snapshot
        .effects
        .values()
        .filter(|e| e.invocation.descriptor.handler.name == "native.story.create")
        .filter_map(|e| match &e.observation {
            Some(super::neutral_effects::EffectObservation {
                stopped: true,
                outcome:
                    super::neutral_effects::EffectOutcome::Completed {
                        result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
                    },
            }) => {
                let receipt = &outputs["receipt"];
                Some((
                    (e.key.fence.activation, e.key.fence.attempt),
                    Created {
                        id: receipt["story"]["id"].as_str()?.to_owned(),
                        revision: receipt["story"]["revision"].as_i64()?,
                        plan: receipt["story"]["planId"]
                            .as_str()
                            .or(receipt["story"]["plan_id"].as_str())?
                            .to_owned(),
                        review: receipt["review"].as_str()?.to_owned(),
                    },
                ))
            }
            _ => None,
        })
        .collect();
    created.sort_by_key(|(order, _)| *order);
    created.into_iter().map(|(_, c)| c).collect()
}

/// A run-time cohort (decision a, 2026-11-12) names only stories this run created:
/// each member has a settled native.story.create receipt in the bound run. Plan,
/// source and exact current revision are checked by `Cohort::current`.
fn created_in_run(context: &NativeContext, cohort: &Cohort) -> Result<(), String> {
    let (run, store) = context
        .bound_run()
        .ok_or("run-time cohort needs the bound run's creation receipts")?;
    let created: BTreeSet<String> = created_stories(&store.neutral_execution(run)?)
        .into_iter()
        .map(|c| c.id)
        .collect();
    if let Some(foreign) = cohort.stories.iter().find(|s| !created.contains(&s.id)) {
        return Err(format!(
            "run-time cohort member {} was not created by this run",
            foreign.id
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatedRequest {
    pub plan: String,
    pub review: String,
}

/// native.story.cohort.created (decision B(a), 2026-11-12): the cohort of stories
/// this run created under `review`, in creation order. A ledger read; the plan's
/// revision and source are read (not written) from the selected backend.
pub(super) fn created_cohort(
    context: &NativeContext,
    store: &super::RunStore,
    key: &EffectKey,
    backend: &dyn StoryBackend,
    request: &CreatedRequest,
) -> Result<Cohort, String> {
    context.current(None)?;
    let stories: Vec<StoryPin> = created_stories(&store.neutral_execution(&key.fence.run)?)
        .into_iter()
        .filter(|c| c.plan == request.plan && c.review == request.review)
        .map(|c| StoryPin {
            id: c.id,
            revision: c.revision,
        })
        .collect();
    if stories.is_empty() {
        return Err("this run created no story under that plan and review".into());
    }
    let plan = backend.require_unarchived_plan(&request.plan)?;
    let cohort = Cohort {
        plan: plan.id,
        plan_revision: plan.revision,
        source: plan.source,
        stories,
    };
    cohort.validate()?;
    Ok(cohort)
}

/// The created-cohort receipt: the cohort, plus one member item per story for a
/// ForEach body (which receives only its item): the cohort, the story pin and the
/// cohort of that one story for its child dispatch.
pub(super) fn created_receipt(cohort: &Cohort) -> serde_json::Value {
    let members: Vec<_> = cohort
        .stories
        .iter()
        .map(|story| {
            let single = Cohort {
                stories: vec![story.clone()],
                ..cohort.clone()
            };
            serde_json::json!({"cohort": cohort, "story": story, "single": single})
        })
        .collect();
    serde_json::json!({"cohort": cohort, "members": members})
}
