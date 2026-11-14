//! Owned proposal creation wraps the selected story backend; reservations never replay writes.
use super::neutral_effects::EffectKey;
use super::{RunStore, native_git::NativeFailure, native_operations::NativeContext};
use crate::stories::{NewStory, Story, StoryBackend};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateRequest {
    /// Approved review/coordinator scope, unique within this run.
    pub review: String,
    pub cap: u32,
    pub source_story: Option<String>,
    pub source_revision: Option<i64>,
    pub evidence: Vec<String>,
    pub proposal: String,
    pub story: NewStory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CreateReceipt {
    pub story: Story,
    pub responsible: String,
    pub review: String,
    pub proposal: String,
    pub effect: EffectKey,
    pub source: CreateRequest,
}

pub(super) fn validate(request: &CreateRequest) -> Result<(), String> {
    if request
        .problem
        .as_ref()
        .is_some_and(|problem| problem.trim().is_empty() || problem.chars().count() > 20_000)
    {
        return Err("native proposal problem must contain at most 20000 characters".into());
    }
    if request.review.trim().is_empty()
        || request.review.len() > 128
        || request.proposal.trim().is_empty()
        || request.proposal.len() > 128
        || request.evidence.len() > 100
        || request.evidence.iter().any(|s| s.len() > 4096)
        || request.cap > 100
    {
        return Err("invalid native proposal scope or cap".into());
    }
    Ok(())
}

pub(super) fn create(
    context: &NativeContext,
    ledger: &RunStore,
    key: &EffectKey,
    backend: &dyn StoryBackend,
    request: &CreateRequest,
) -> Result<CreateReceipt, NativeFailure> {
    context.authorize(ledger, key, "native.story.create", request, None)?;
    validate(request)?;
    let plan = backend.require_unarchived_plan(&request.story.plan_id)?;
    if plan.project != context.workspace_path().to_string_lossy() {
        return Err("native proposal plan belongs to another project"
            .to_string()
            .into());
    }
    if let Some(id) = &request.source_story {
        let source = backend.require_unarchived_story(id)?;
        if source.plan_id != request.story.plan_id
            || Some(source.revision) != request.source_revision
        {
            return Err("native proposal source story/version is stale"
                .to_string()
                .into());
        }
    } else if request.source_revision.is_some() {
        return Err("native proposal source revision lacks a story"
            .to_string()
            .into());
    }
    reserve(context, ledger, key, request)?;
    // This service uses the configured external adapter's create_story when selected.
    // A lost reply or ledger failure is uncertain, never permission to call it again.
    let proposal = crate::workflows::catalog::content_digest(&serde_json::json!([
        request.review,
        request.proposal
    ]))?;
    let story = backend
        .create_story_with_problem_once(
            &key.fence.run,
            &proposal,
            request.story.clone(),
            request.problem.clone(),
        )
        .map_err(NativeFailure::Uncertain)?;
    Ok(CreateReceipt {
        story,
        responsible: context.caller().into(),
        review: request.review.clone(),
        proposal: request.proposal.clone(),
        effect: key.clone(),
        source: request.clone(),
    })
}

fn reserve(
    context: &NativeContext,
    ledger: &RunStore,
    key: &EffectKey,
    request: &CreateRequest,
) -> Result<(), String> {
    let mut conn = Connection::open(ledger.database_path()).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_story_proposal_caps (
        run TEXT NOT NULL, review TEXT NOT NULL, cap INTEGER NOT NULL,
        PRIMARY KEY(run, review));
        CREATE TABLE IF NOT EXISTS native_story_proposal_reservations (
        run TEXT NOT NULL, review TEXT NOT NULL, proposal TEXT NOT NULL,
        responsible TEXT NOT NULL, payload TEXT NOT NULL, effect TEXT NOT NULL,
        PRIMARY KEY(run, review, proposal));",
    )
    .map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let cap: Option<u32> = tx
        .query_row(
            "SELECT cap FROM native_story_proposal_caps WHERE run=?1 AND review=?2",
            params![key.fence.run, request.review],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if cap.is_some_and(|cap| cap != request.cap) {
        return Err("native review proposal cap cannot change".into());
    }
    tx.execute(
        "INSERT OR IGNORE INTO native_story_proposal_caps VALUES (?1,?2,?3)",
        params![key.fence.run, request.review, request.cap],
    )
    .map_err(|e| e.to_string())?;
    let count: u32 = tx
        .query_row(
            "SELECT count(*) FROM native_story_proposal_reservations WHERE run=?1 AND review=?2",
            params![key.fence.run, request.review],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if count >= request.cap {
        tx.commit().map_err(|e| e.to_string())?;
        return Err("native review proposal cap exhausted".into());
    }
    // The unique reservation refuses retries, including ambiguous external writes.
    tx.execute(
        "INSERT INTO native_story_proposal_reservations VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            key.fence.run,
            request.review,
            request.proposal,
            context.caller(),
            serde_json::to_string(request).map_err(|e| e.to_string())?,
            serde_json::to_string(key).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| format!("proposal already reserved or unavailable: {e}"))?;
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "native_story_create_tests.rs"]
mod tests;

/// Observe only the original backend identity; never resend creation.
pub(super) fn reconcile(
    context: &NativeContext,
    store: &RunStore,
    effect: &super::neutral_effects::NeutralEffect,
) -> Result<CreateReceipt, String> {
    let backend = crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?;
    reconcile_with(context, store, effect, backend.as_ref())
}

fn reconcile_with(
    context: &NativeContext,
    store: &RunStore,
    effect: &super::neutral_effects::NeutralEffect,
    backend: &dyn StoryBackend,
) -> Result<CreateReceipt, String> {
    context.current(None)?;
    let retained = store.neutral_execution(&effect.key.fence.run)?;
    if retained.effect(&effect.key)? != effect {
        return Err("native proposal reconciliation lacks original effect".into());
    }
    let request: CreateRequest =
        serde_json::from_value(effect.invocation.inputs["request"].clone())
            .map_err(|e| e.to_string())?;
    let conn = Connection::open(store.database_path()).map_err(|e| e.to_string())?;
    let (responsible, payload, original): (String, String, String) = conn.query_row(
        "SELECT responsible,payload,effect FROM native_story_proposal_reservations WHERE run=?1 AND review=?2 AND proposal=?3",
        params![effect.key.fence.run, request.review, request.proposal],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).map_err(|e| e.to_string())?;
    if payload != serde_json::to_string(&request).map_err(|e| e.to_string())?
        || original != serde_json::to_string(&effect.key).map_err(|e| e.to_string())?
        || responsible != context.caller()
    {
        return Err("native proposal identity differs from reservation".into());
    }
    let proposal = crate::workflows::catalog::content_digest(&serde_json::json!([
        request.review,
        request.proposal
    ]))?;
    let story = backend
        .existing_story_with_problem_for_proposal(
            &effect.key.fence.run,
            &proposal,
            &request.story,
            request.problem.as_deref(),
        )?
        .ok_or("original story creation outcome cannot be proven; disposition required")?;
    Ok(CreateReceipt {
        story,
        responsible,
        review: request.review.clone(),
        proposal: request.proposal.clone(),
        effect: effect.key.clone(),
        source: request,
    })
}
