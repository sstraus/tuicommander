use super::super::neutral_agent_adapter::{RESTRICTED_COARSE_UNAVAILABLE, restricted_agent};
use super::super::neutral_effects::EffectOutcome;
use super::super::neutral_execution::NeutralResult;

// Catches reporting success or live process ownership when restricted execution is unavailable.
#[test]
fn coarse_restricted_route_is_explicitly_unavailable_without_a_process_or_receipt() {
    let observation = restricted_agent();
    assert!(observation.stopped);
    assert_eq!(
        observation.outcome,
        EffectOutcome::Completed {
            result: NeutralResult::Failed {
                diagnostic: RESTRICTED_COARSE_UNAVAILABLE.into()
            },
        }
    );
}
