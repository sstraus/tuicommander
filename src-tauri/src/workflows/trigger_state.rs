//! Report installed source refusals without changing polling eligibility.
use super::trigger::{NeutralTriggerBinding, TriggerSource};

pub(super) fn binding_state(mut binding: NeutralTriggerBinding) -> NeutralTriggerBinding {
    binding.refusal_reason = None;
    if binding.enabled
        && binding.draft.source == TriggerSource::Github
        && binding.draft.kind == "issue"
    {
        binding.refusal_reason = crate::github::issue_intake_poll_sources()
            .err()
            .map(|error| format!("GitHub issue source unavailable: {error}"));
    }
    binding
}
