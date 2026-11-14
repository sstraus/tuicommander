//! Fence business-result consumption against the retained source generation.
//! Called inside the same immediate transaction as the neutral event append.
use super::neutral_effects::EffectDecision;
use super::neutral_execution::{NeutralDecision, NeutralResult};
use super::neutral_trigger_store::TriggerIntent;
use crate::workflows::correction::CorrectionArtifact;
use rusqlite::{Connection, OptionalExtension};

fn retained(conn: &Connection, run: &str) -> Result<Option<(TriggerIntent, bool)>, String> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='neutral_trigger_intents')",
        [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !exists {
        return Ok(None);
    }
    conn.query_row(
        "SELECT i.document, r.active_run=i.run_id AND r.pending_receipt IS NULL
        FROM neutral_trigger_intents i JOIN neutral_trigger_resources r
        ON r.project=i.project AND r.binding_id=i.binding_id AND r.resource=i.resource
        WHERE i.run_id=?1",
        [run],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())?
    .map(|(document, current)| {
        serde_json::from_str(&document)
            .map(|intent| (intent, current))
            .map_err(|e| e.to_string())
    })
    .transpose()
}

pub(super) fn require_artifact(
    store: &super::RunStore,
    run: &str,
    artifact: &CorrectionArtifact,
) -> Result<(), String> {
    let Some((intent, current)) = retained(&store.connect()?, run)? else {
        return Ok(());
    };
    if !current {
        return Err("correction source generation superseded".into());
    }
    if intent.event.kind == "pull_request"
        && (intent.event.values.get("head").and_then(|v| v.as_str())
            != Some(artifact.head.as_str())
            || intent.event.values.get("base").and_then(|v| v.as_str())
                != Some(artifact.base.as_str()))
    {
        return Err("correction artifact differs from pinned trigger head/base".into());
    }
    Ok(())
}

/// True requests a retained Cancel before retrying this effect observation.
/// Decisions must remain pure functions of retained commands for exact replay.
pub(super) fn fence(
    conn: &Connection,
    run: &str,
    decision: &NeutralDecision,
) -> Result<bool, String> {
    let Some((_, current)) = retained(conn, run)? else {
        return Ok(false);
    };
    if current {
        return Ok(false);
    }
    match decision {
        NeutralDecision::Complete {
            result: NeutralResult::Completed { .. },
            ..
        }
        | NeutralDecision::Finish { .. } => Err("correction source generation superseded".into()),
        NeutralDecision::Effect { decision } => {
            if let EffectDecision::Observe {
                result: Some(result),
                ..
            } = decision.as_ref()
            {
                // Keep the original successful receipt/stop observation. Cancel
                // first so its ordinary replayable callback cannot advance work.
                return Ok(fence(conn, run, result).is_err());
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// Exact normalized generation; manually started runs cannot invent one.
pub(super) fn require_source(
    store: &super::RunStore,
    run: &str,
    generation: Option<&str>,
) -> Result<(), String> {
    match retained(&store.connect()?, run)? {
        Some((intent, true)) if generation == Some(intent.event.generation.as_str()) => Ok(()),
        None if generation.is_none() => Ok(()),
        _ => Err("operator source generation is stale or differs from retained source".into()),
    }
}
