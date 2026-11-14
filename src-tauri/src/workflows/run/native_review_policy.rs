//! Reviewer admissions are durable service policy, independent of loop retries.
use super::{RunStore, native_operations::NativeContext, neutral_effects::EffectKey};
use crate::workflows::{catalog::ActivityInstance, review_policy::ReviewAdmissionPolicy};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) const HANDLER: &str = "native.review.bounded";

pub(super) fn policy(instance: &ActivityInstance) -> Result<ReviewAdmissionPolicy, String> {
    let policy: ReviewAdmissionPolicy = serde_json::from_value(
        instance
            .config
            .get("policy")
            .cloned()
            .ok_or("review admission policy missing")?,
    )
    .map_err(|e| e.to_string())?;
    policy.validate()?;
    Ok(policy)
}

fn connect(store: &RunStore) -> Result<Connection, String> {
    let conn = Connection::open(store.database_path()).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_review_policies (
        run TEXT NOT NULL, scope TEXT NOT NULL, policy TEXT NOT NULL, PRIMARY KEY(run,scope));
        CREATE TABLE IF NOT EXISTS native_review_admissions (
        run TEXT NOT NULL, scope TEXT NOT NULL, effect TEXT NOT NULL, ordinal INTEGER NOT NULL,
        PRIMARY KEY(run,scope,effect), UNIQUE(run,scope,ordinal));",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// A ticket is spent immediately before entering the native independent reviewer.
/// Even a lost provider reply cannot authorize re-entry under that same effect.
pub(super) fn reserve(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
) -> Result<u32, String> {
    context.current(None)?;
    let run = store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if !matches!(
        effect.invocation.descriptor.handler.name.as_str(),
        HANDLER | super::native_agent::HANDLER
    ) || !effect.dispatched
        || effect.cancel_started
        || effect.observation.is_some()
        || run.status != super::neutral_execution::NeutralStatus::Running
        || run.deadline_ms <= super::store::now_ms()
    {
        return Err("review admission lacks current owned effect".into());
    }
    let policy = policy(&effect.invocation.instance)?;
    let value = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
    let mut conn = connect(store)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let old: Option<String> = tx
        .query_row(
            "SELECT policy FROM native_review_policies WHERE run=?1 AND scope=?2",
            params![key.fence.run, policy.scope],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if old.as_ref().is_some_and(|old| old != &value) {
        return Err("review admission policy cannot change within run/scope".into());
    }
    tx.execute(
        "INSERT OR IGNORE INTO native_review_policies VALUES (?1,?2,?3)",
        params![key.fence.run, policy.scope, value],
    )
    .map_err(|e| e.to_string())?;
    let count: u32 = tx
        .query_row(
            "SELECT count(*) FROM native_review_admissions WHERE run=?1 AND scope=?2",
            params![key.fence.run, policy.scope],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if count >= u32::from(policy.evidence.max_reviews) {
        return Err("independent reviewer admission budget exhausted".into());
    }
    let ordinal = count + 1;
    tx.execute(
        "INSERT INTO native_review_admissions VALUES (?1,?2,?3,?4)",
        params![key.fence.run, policy.scope, key.digest, ordinal],
    )
    .map_err(|_| "review already admitted; reconcile original provider result".to_owned())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ordinal)
}

pub(super) fn ordinal(
    store: &RunStore,
    key: &EffectKey,
    policy: &ReviewAdmissionPolicy,
) -> Result<u32, String> {
    let conn = connect(store)?;
    conn.query_row("SELECT a.ordinal FROM native_review_admissions a JOIN native_review_policies p USING(run,scope)
        WHERE a.run=?1 AND a.scope=?2 AND a.effect=?3 AND p.policy=?4",
        params![key.fence.run,policy.scope,key.digest,serde_json::to_string(policy).map_err(|e|e.to_string())?],|r|r.get(0))
        .map_err(|_|"review lacks its retained policy admission".into())
}

pub(super) fn accounting(
    store: &RunStore,
    run: &super::neutral_execution::NeutralExecution,
    policy: &ReviewAdmissionPolicy,
) -> Result<serde_json::Value, String> {
    let conn = connect(store)?;
    let admitted: u32 = conn
        .query_row(
            "SELECT count(*) FROM native_review_admissions WHERE run=?1 AND scope=?2",
            params![run.run.id, policy.scope],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let rejected = run
        .effects
        .values()
        .filter(|e| {
            if self::policy(&e.invocation.instance).as_ref() != Ok(policy) {
                return false;
            }
            let Some(super::neutral_effects::EffectObservation {
                stopped: true,
                outcome:
                    super::neutral_effects::EffectOutcome::Completed {
                        result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
                    },
            }) = &e.observation
            else {
                return false;
            };
            if e.invocation.descriptor.handler.name == HANDLER {
                outputs
                    .get("receipt")
                    .and_then(|v| serde_json::from_value::<super::ReviewReceipt>(v.clone()).ok())
                    .is_some_and(|r| {
                        r.verdict == crate::workflows::review_verdict::CodeVerdict::Findings
                    })
            } else if e.invocation.descriptor.handler.name == super::native_agent::HANDLER {
                outputs
                    .get("report")
                    .and_then(|v| {
                        serde_json::from_value::<super::evidence::ReviewEvidence>(
                            v["report"]["evidence"].clone(),
                        )
                        .ok()
                    })
                    .is_some_and(|e| {
                        e.findings
                            .iter()
                            .any(|f| f.decision() == super::evidence::TriageDecision::Fix)
                    })
            } else {
                false
            }
        })
        .count();
    let proposals = run
        .effects
        .values()
        .filter(|e| e.invocation.descriptor.handler.name == "native.story.create" && e.settled())
        .count();
    let checks = run
        .effects
        .values()
        .filter(|e| {
            e.dispatched && e.invocation.descriptor.handler.name == "native.verification.run"
        })
        .count() as u64
        + super::native_policy_checks::count(store, &run.run.id, &policy.scope)?;
    let mut corrections = 0;
    let mut current = Some(run);
    while let Some(run) = current {
        if let Some(scope) = &run.active_scope
            && scope.identity.scope == "correction"
            && let super::neutral_scopes::NeutralScopeKind::Loop { iteration, .. } = scope.kind
        {
            corrections = corrections.max(iteration.saturating_sub(1));
        }
        current = run.parent.as_deref();
    }
    Ok(
        serde_json::json!({"scope":policy.scope,"max_reviews":policy.evidence.max_reviews,
        "max_repairs":policy.evidence.max_repairs,"reviews":admitted,"rejected_reviews":rejected,
        "corrections":corrections,"check_attempts":checks,"proposals":proposals}),
    )
}
