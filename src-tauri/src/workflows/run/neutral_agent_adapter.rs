//! Restricted coarse execution is unavailable without native privilege withholding.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "native host installation belongs to 008-22c4")
)]
use super::neutral_effects::{EffectObservation, EffectOutcome};
use super::neutral_execution::NeutralResult;

pub(super) const RESTRICTED_COARSE_UNAVAILABLE: &str = "restricted_execution_unavailable: native workspace and direct service privilege withholding is unavailable";

/// No process or session is started. Native installation must not substitute
/// ordinary agent launchers, whose Git guards do not withhold direct privilege.
pub(super) fn restricted_agent() -> EffectObservation {
    EffectObservation {
        stopped: true,
        outcome: EffectOutcome::Completed {
            result: NeutralResult::Failed {
                diagnostic: RESTRICTED_COARSE_UNAVAILABLE.into(),
            },
        },
    }
}
