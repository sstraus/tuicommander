//! Batch joins accept only owned native evidence of this run, never caller claims.
use super::native_batch::{
    self as batch, AdmitRequest, Critic, LandRequest, PendingRequest, PinRequest, VerdictRequest,
};
use super::native_host::NativeBinding;
use super::neutral_effects::{EffectKey, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use serde::Deserialize;
use serde_json::Value;

const PUSH: &str = "native.publication.push";
const COMMIT: &str = "native.commit.create";

/// Push and commit effects this run already holds; none can land a later admission.
pub(super) fn known_pushes(run: &NeutralExecution) -> Vec<String> {
    run.effects
        .values()
        .filter(|e| matches!(e.invocation.descriptor.handler.name.as_str(), PUSH | COMMIT))
        .map(|e| e.key.digest.clone())
        .collect()
}

/// The commit of a settled, completed native push or commit owned by this run.
pub(super) fn landing(run: &NeutralExecution, push: &EffectKey) -> Result<String, String> {
    if push.fence.run != run.run.id {
        return Err("batch landing push belongs to a foreign run".into());
    }
    let effect = run.effect(push)?;
    // Decision 1(i), 2026-11-12: a commit of this run on the same workspace also
    // lands its member; the pin proves it is in the clean HEAD.
    let handler = effect.invocation.descriptor.handler.name.as_str();
    if !matches!(handler, PUSH | COMMIT) {
        return Err("batch landing requires a native push or commit effect".into());
    }
    let Some(EffectObservation {
        stopped: true,
        outcome:
            EffectOutcome::Completed {
                result: NeutralResult::Completed { outputs, .. },
            },
    }) = &effect.observation
    else {
        return Err("batch landing push has no completed stopped observation".into());
    };
    let receipt = outputs
        .get("receipt")
        .cloned()
        .ok_or("batch landing receipt missing")?;
    Ok(if handler == PUSH {
        serde_json::from_value::<super::native_git::PushReceipt>(receipt)
            .map_err(|e| format!("batch landing push receipt: {e}"))?
            .commit
    } else {
        serde_json::from_value::<super::native_git::CommitReceipt>(receipt)
            .map_err(|e| format!("batch landing commit receipt: {e}"))?
            .commit
    })
}

/// Decision G1 (2026-11-12): the member's landing is the latest settled push or
/// commit of this run that was not known at its admission. No caller key.
pub(super) fn select_landing(
    run: &NeutralExecution,
    known: &[String],
) -> Result<(String, String), String> {
    let mut candidates: Vec<_> = run
        .effects
        .values()
        .filter(|e| matches!(e.invocation.descriptor.handler.name.as_str(), PUSH | COMMIT))
        .filter(|e| !known.contains(&e.key.digest))
        .filter_map(|e| landing(run, &e.key).ok().map(|commit| (e, commit)))
        .collect();
    candidates.sort_by_key(|(e, _)| (e.key.fence.activation, e.key.fence.attempt));
    let (effect, commit) = candidates
        .pop()
        .ok_or("no settled push or commit of this run since the member's admission")?;
    Ok((effect.key.digest.clone(), commit))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Approve,
    Fix,
}
// Extra role output fields (summary, findings) are allowed; these two decide.
#[derive(Deserialize)]
struct CriticReport {
    decision: Decision,
    #[serde(default)]
    story: Option<String>,
}

/// One source-bound reviewer/validator report of this run over its source artifact.
pub(super) fn critic(
    bound: &NativeBinding,
    run: &str,
    report: &EffectKey,
    source: &super::native_agent::Source,
) -> Result<Critic, String> {
    if report.fence.run != run {
        return Err("batch critic report belongs to a foreign run".into());
    }
    let receipt = super::native_agent::read_report(bound, report, source)?;
    if !matches!(receipt["role"].as_str(), Some("reviewer" | "validator")) {
        return Err("batch critic must be an independent reviewer or validator role".into());
    }
    let body: CriticReport = serde_json::from_value(receipt["report"].clone())
        .map_err(|e| format!("batch critic report: {e}"))?;
    let fix = match (body.decision, body.story) {
        (Decision::Approve, None) => None,
        (Decision::Fix, Some(story)) => Some(story),
        (Decision::Approve, Some(_)) => {
            return Err("an approving batch review names no FIX owner".into());
        }
        (Decision::Fix, None) => {
            return Err("a batch FIX without an owning story is refused".into());
        }
    };
    Ok(Critic {
        report: report.digest.clone(),
        commit: source.commit.clone(),
        tree: source.tree.clone(),
        fix,
    })
}

fn request<T: serde::de::DeserializeOwned>(effect: &NeutralEffect) -> Result<T, String> {
    serde_json::from_value(
        effect
            .invocation
            .inputs
            .get("request")
            .cloned()
            .ok_or("native request absent")?,
    )
    .map_err(|e| format!("native request: {e}"))
}

/// Build member inputs from the real backend, preserving the retained full
/// cohort while refreshing only each child's exact admission revision.
pub(super) fn pending_receipt(
    context: &super::native_operations::NativeContext,
    store: &super::RunStore,
    run: &str,
    backend: &dyn crate::stories::StoryBackend,
    r: &PendingRequest,
) -> Result<Value, String> {
    let current = r.cohort.current(context, backend, false)?;
    let pending = batch::pending(store, run, &r.cohort, r.limit)?;
    let members: Vec<_> = pending
        .into_iter()
        .map(|story| {
            let revision = current
                .iter()
                .find(|s| s.id == story.id)
                .ok_or("pending story is missing from the current backend")?
                .revision;
            let story = super::native_cohort::StoryPin { revision, ..story };
            let single = super::native_cohort::Cohort {
                stories: vec![story.clone()],
                ..r.cohort.clone()
            };
            Ok(serde_json::json!({"cohort": r.cohort, "story": story, "single": single}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(serde_json::json!({"cohort": r.cohort, "members": members}))
}

pub(super) fn execute(bound: &NativeBinding, effect: &NeutralEffect) -> Result<Value, String> {
    let (store, key) = (&bound.store, &effect.key);
    let run = key.fence.run.as_str();
    if let Some(receipt) = batch::retained(store, run, &key.digest)? {
        return Ok(receipt);
    }
    match effect.invocation.descriptor.handler.name.as_str() {
        batch::PENDING => {
            let r: PendingRequest = request(effect)?;
            let backend = crate::stories::story_backend_for(
                &bound.context.workspace_path().to_string_lossy(),
            )?;
            let receipt = pending_receipt(&bound.context, store, run, backend.as_ref(), &r)?;
            batch::retain_pending(store, run, &key.digest, receipt)
        }
        batch::ADMIT => {
            let r: AdmitRequest = request(effect)?;
            let backend = crate::stories::story_backend_for(
                &bound.context.workspace_path().to_string_lossy(),
            )?;
            r.cohort.current(&bound.context, backend.as_ref(), false)?;
            let known = known_pushes(&store.neutral_execution(run)?);
            batch::admit(store, run, &key.digest, &r.cohort, &r.story, &known)
        }
        batch::LAND => {
            let r: LandRequest = request(effect)?;
            let known = batch::known_at_admission(store, run, &r.plan, &r.story)?;
            let (landing, commit) = select_landing(&store.neutral_execution(run)?, &known)?;
            batch::land(
                store,
                run,
                &key.digest,
                &r.plan,
                &r.story,
                &landing,
                &commit,
            )
        }
        batch::PIN => {
            let r: PinRequest = request(effect)?;
            batch::pin(
                store,
                run,
                &key.digest,
                &r.plan,
                bound.context.workspace_path(),
            )
        }
        batch::VERDICT => {
            let r: VerdictRequest = request(effect)?;
            let critic = critic(bound, run, &r.report, &r.source)?;
            batch::verdict(store, run, &key.digest, &r.plan, r.generation, &critic)
        }
        handler => Err(format!("native batch handler unavailable: {handler}")),
    }
}
