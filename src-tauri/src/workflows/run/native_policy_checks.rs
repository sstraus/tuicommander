//! Assessment checks retain their original attempt/receipt across refresh and loss.
use super::check::CheckReceipt;
use super::native_git::NativeFailure;
use super::{RunStore, native_operations::NativeContext, neutral_effects::EffectKey};
use rusqlite::{Connection, OptionalExtension, params};

fn connect(store: &RunStore) -> Result<Connection, String> {
    let conn = Connection::open(store.database_path()).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_policy_checks (
        run TEXT NOT NULL, scope TEXT NOT NULL, effect TEXT NOT NULL, check_id TEXT NOT NULL,
        receipt TEXT, PRIMARY KEY(run,effect,check_id));",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

pub(super) fn verify(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    check: &str,
    commit: &str,
    tree: &str,
) -> Result<CheckReceipt, NativeFailure> {
    context.require_artifact(commit, tree)?;
    let run = store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if effect.invocation.descriptor.handler.name != super::correction_policy::HANDLER {
        return Err("policy check requires its assessment effect"
            .to_owned()
            .into());
    }
    let policy = super::native_review_policy::policy(&effect.invocation.instance)?;
    let conn = connect(store)?;
    let prior: Option<Option<String>> = conn
        .query_row(
            "SELECT receipt FROM native_policy_checks WHERE run=?1 AND effect=?2 AND check_id=?3",
            params![key.fence.run, key.digest, check],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match prior {
        Some(Some(value)) => {
            let receipt:CheckReceipt=serde_json::from_str(&value).map_err(|e|e.to_string())?;
            if receipt.commit!=commit || receipt.tree!=tree || receipt.check_id!=check {
                return Err("retained check receipt does not cover current artifact".to_owned().into());
            }
            return Ok(receipt);
        }
        Some(None) => return Err(NativeFailure::Uncertain("assessment check reply is missing; reconcile its original process, never execute again".into())),
        None => {}
    }
    if !effect.dispatched
        || effect.cancel_started
        || effect.observation.is_some()
        || run.status != super::neutral_execution::NeutralStatus::Running
    {
        return Err("policy check assessment is no longer current"
            .to_owned()
            .into());
    }
    // An insert is the one admission. A concurrent callback cannot run again.
    conn.execute(
        "INSERT INTO native_policy_checks VALUES (?1,?2,?3,?4,NULL)",
        params![key.fence.run, policy.scope, key.digest, check],
    )
    .map_err(|_| {
        NativeFailure::Uncertain(
            "assessment check already admitted; reconcile original attempt".into(),
        )
    })?;
    let receipt = context
        .verify_observed(check, commit, tree, store.database_path(), &key.fence.run)
        .map_err(NativeFailure::Uncertain)?;
    let value =
        serde_json::to_string(&receipt).map_err(|e| NativeFailure::Uncertain(e.to_string()))?;
    conn.execute("UPDATE native_policy_checks SET receipt=?1 WHERE run=?2 AND effect=?3 AND check_id=?4 AND receipt IS NULL",
        params![value,key.fence.run,key.digest,check]).map_err(|e|NativeFailure::Uncertain(e.to_string()))?;
    Ok(receipt)
}

pub(super) fn count(store: &RunStore, run: &str, scope: &str) -> Result<u64, String> {
    let count: i64 = connect(store)?
        .query_row(
            "SELECT count(*) FROM native_policy_checks WHERE run=?1 AND scope=?2",
            params![run, scope],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    u64::try_from(count).map_err(|e| e.to_string())
}

pub(super) fn assessment(
    context: &NativeContext,
    store: &RunStore,
    effect: &super::neutral_effects::NeutralEffect,
    check: &str,
    artifact: &crate::workflows::correction::CorrectionArtifact,
) -> Result<CheckReceipt, NativeFailure> {
    if effect.invocation.descriptor.handler.name == super::correction_policy::HANDLER {
        verify(
            context,
            store,
            &effect.key,
            check,
            &artifact.head,
            &artifact.material_digest,
        )
    } else {
        context
            .verify_observed(
                check,
                &artifact.head,
                &artifact.material_digest,
                store.database_path(),
                &effect.key.fence.run,
            )
            .map_err(NativeFailure::Refused)
    }
}
