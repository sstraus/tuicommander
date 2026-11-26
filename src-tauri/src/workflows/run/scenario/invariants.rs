//! Invariants checked after every fault execution.
use super::faults::{Fault, Outcome};
use super::rig::Rig;
use super::run::neutral_execution::{NeutralExecution, NeutralStatus};
use std::collections::BTreeMap;

pub(super) struct Evidence<'a> {
    pub outcome: &'a Outcome,
    pub golden: Option<&'a NeutralExecution>,
    pub canon: fn(&NeutralExecution) -> serde_json::Value,
}

fn claims(rig: &Rig, run: &str, effect: &str) -> i64 {
    rig.owner
        .store
        .connect()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM neutral_effect_resources WHERE run_id=?1 AND effect_id=?2",
            rusqlite::params![run, effect],
            |r| r.get(0),
        )
        .unwrap()
}

/// No resource release before stop evidence: while a mutating effect was
/// dispatched and its stop is unproven, its resource claim must still exist.
fn claims_held(rig: &Rig, run: &NeutralExecution, label: &str) {
    for effect in run.effects.values() {
        let stopped = effect.observation.as_ref().is_some_and(|o| o.stopped);
        if effect.dispatched
            && effect.preparation.mutation
            && !stopped
            && !effect.preparation.resources.is_empty()
        {
            assert!(
                claims(rig, &run.run.id, &effect.key.digest) > 0,
                "{label}: resource released before stop evidence for {}",
                effect.key.digest
            );
        }
    }
}

pub(super) fn check_frozen(rig: &Rig, run: &NeutralExecution, fault: Fault) {
    claims_held(rig, run, &format!("{fault:?} at crash point"));
}

fn kind(event: &serde_json::Value) -> (&str, &serde_json::Value) {
    let decision = &event["kind"]["decision"];
    (decision["type"].as_str().unwrap_or(""), decision)
}

pub(super) fn check_final(rig: &Rig, evidence: &Evidence) {
    let Outcome {
        fault, run, events, ..
    } = evidence.outcome;
    let label = format!("{fault:?}");
    // No second external write: one adapter entry per mutating effect, one dispatch event per key.
    let mut entries: BTreeMap<String, u32> = BTreeMap::new();
    for call in rig.calls.entries.lock().iter().filter(|c| c.mutation) {
        *entries.entry(call.digest.clone()).or_default() += 1;
    }
    assert!(
        entries.values().all(|n| *n == 1),
        "{label}: a mutating effect reached the adapter twice: {entries:?}"
    );
    let mut dispatches: BTreeMap<String, u32> = BTreeMap::new();
    for event in events {
        let (name, decision) = kind(event);
        if name == "effect" && decision["decision"]["type"] == "dispatch" {
            *dispatches
                .entry(
                    decision["decision"]["key"]["digest"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned(),
                )
                .or_default() += 1;
        }
    }
    // Only an operator-approved rerun of a read-only script dispatches a key again.
    assert!(
        dispatches.iter().all(|(digest, n)| {
            let reruns = run.effects.get(digest).map_or(0, |e| e.reruns);
            *n == 1 || (!run.effects[digest].preparation.mutation && *n == 1 + reruns)
        }),
        "{label}: duplicate dispatch {dispatches:?}"
    );
    // The graph never advances past an activity without a committed completion.
    // Pending activation per execution frame; entering a subflow hands it to the child.
    let mut pending: BTreeMap<String, u64> = BTreeMap::new();
    for event in events {
        let (name, decision) = kind(event);
        let result = &decision["decision"]["result"];
        let fence = if name == "effect" && result["type"] == "complete" {
            Some(&result["fence"])
        } else if matches!(name, "complete" | "prepare" | "enter_subflow") {
            Some(&decision["fence"])
        } else {
            None
        };
        let frame = fence.map(|f| f["execution"].as_str().unwrap_or("").to_owned());
        let activation = fence.and_then(|f| f["activation"].as_u64());
        match (name, &frame) {
            ("prepare", Some(frame)) => {
                pending.insert(frame.clone(), activation.unwrap());
            }
            ("complete" | "enter_subflow", Some(frame)) | ("effect", Some(frame)) => {
                if pending.get(frame) == activation.as_ref() {
                    pending.remove(frame);
                }
            }
            ("route" | "finish", _) => {
                // A route belongs to the innermost frame still executing an activity.
                assert!(
                    pending.is_empty(),
                    "{label}: {name} at seq {} while {pending:?} has no completion",
                    event["sequence"]
                );
            }
            _ => {}
        }
    }
    // Nothing is dispatched after a cancel commit.
    if let Some(cancel) = events.iter().position(|e| kind(e).0 == "cancel") {
        assert!(
            events[cancel..]
                .iter()
                .all(|e| kind(e).1["decision"]["type"] != "dispatch"),
            "{label}: dispatch after cancel"
        );
    }
    claims_held(rig, run, &label);
    // Terminal, or an open approval (the documented waiting state).
    // An uncertain effect with no restart waits for reconciliation; that is documented waiting.
    let waiting = !rig.bound(&run.run.id)
        || run.effects.values().any(|e| {
            matches!(
                e.observation.as_ref().map(|o| &o.outcome),
                Some(super::run::neutral_effects::EffectOutcome::Uncertain { .. })
            )
        })
        || run
            .effects
            .values()
            .any(|e| e.question.is_some() && e.answer.is_none());
    assert!(
        run.status != NeutralStatus::Running || waiting,
        "{label}: run neither terminal nor waiting for an operator: seq {} node {} status {:?} events {} bound {}",
        run.sequence,
        run.node,
        run.status,
        events.len(),
        super::run::neutral_runtime::NeutralHost::bound(&*rig.host, &run.run.id)
    );
    match fault {
        Fault::Cancel(_) | Fault::LateCallback(_) if evidence.outcome.reached => {
            assert_eq!(run.status, NeutralStatus::Cancelled, "{label}");
        }
        _ => {
            if let Some(golden) = evidence
                .golden
                .filter(|_| run.status != NeutralStatus::Running)
            {
                assert_eq!(
                    run.status, golden.status,
                    "{label}: outcome differs from the fault-free run"
                );
                assert_eq!(
                    (evidence.canon)(run),
                    (evidence.canon)(golden),
                    "{label}: outputs differ from the fault-free run"
                );
            }
        }
    }
}
