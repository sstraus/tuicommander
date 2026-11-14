use super::neutral_execution::{
    NeutralExecution, NeutralExecutionCommand as Command, NeutralStatus,
};
use super::neutral_execution_tests::step;
use super::runtime::RuntimeOwner;
use crate::workflows::correction::{CorrectionEvaluation, CorrectionState, EvidenceVerdict};
use crate::workflows::correction_tests::{artifact, observation, policy};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(super) fn command(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    command: Command,
    at: i64,
) -> NeutralExecution {
    owner
        .neutral_command_at(
            &run.run.id,
            &format!("command:{}", run.sequence),
            run.sequence,
            command,
            at,
        )
        .unwrap()
}

/// Drive real subflow entry/return and parent setup; stop at one leaf assessment.
pub(super) fn drive(
    owner: &RuntimeOwner,
    mut run: NeutralExecution,
    setup: &mut usize,
    at: i64,
) -> NeutralExecution {
    for _ in 0..100 {
        if run.status != NeutralStatus::Running {
            return run;
        }
        let pending = run.pending_invocations();
        if let Some((activity, attempt)) = pending.first() {
            let descriptor = run.descriptor(*activity).unwrap();
            if descriptor.id.name == "assess" {
                return run;
            }
            let cmd = if descriptor.target.is_some() {
                Command::EnterSubflow {
                    fence: attempt.fence.clone(),
                }
            } else {
                *setup += 1;
                Command::Complete {
                    fence: attempt.fence.clone(),
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(7))]),
                }
            };
            run = command(owner, &run, cmd, at);
        } else {
            run = step(owner, &run, &format!("step:{}", run.sequence), at);
        }
    }
    panic!("correction graph failed to reach next assessment/end");
}

pub(super) fn assessment_outputs(run: &NeutralExecution, failed: bool) -> BTreeMap<String, Value> {
    let attempt = &run.pending_invocations()[0].1;
    let mut state: CorrectionState =
        serde_json::from_value(attempt.inputs["state"].clone()).unwrap();
    let generation = state.artifact.generation;
    let revision = format!("evaluation-{generation}");
    let batch = observation(
        generation,
        &revision,
        if failed {
            EvidenceVerdict::CorrectionRequired
        } else {
            EvidenceVerdict::Passed
        },
    );
    let CorrectionEvaluation::Ready {
        state: assessed,
        repeat,
    } = policy().assess(&state, batch).unwrap()
    else {
        panic!("new assessment unavailable")
    };
    state = *assessed;
    // The internal activity creates the next artifact; scope/count are owned by
    // its parent and cannot be reset by this worker's carried data.
    if failed {
        state.advance_artifact(artifact(generation + 1)).unwrap();
    }
    let mut outputs = BTreeMap::from([("state".into(), json!(state))]);
    if let Some(repeat) = repeat {
        outputs.insert("repeat".into(), json!(repeat));
    }
    outputs
}
