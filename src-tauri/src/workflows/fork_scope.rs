//! Neutral fork/join data contracts. Only the durable run owner releases joins.

// DEFERRED (2026-11-10) — contract entry points await the 004-2809 runtime consumer.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "003-e678 contract; first consumer 004-2809")
)]

use super::collection::{AggregateState, CollectionIdentity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchContract {
    pub activity: usize,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForkContract {
    pub branches: BTreeMap<String, BranchContract>,
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchAttempt {
    pub activation: String,
    pub attempt: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArrivalState {
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchArrival {
    pub identity: CollectionIdentity,
    pub branch: String,
    pub owner: BranchAttempt,
    pub state: ArrivalState,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JoinSnapshot {
    pub identity: CollectionIdentity,
    pub state: AggregateState,
    pub arrivals: BTreeMap<String, ArrivalState>,
}

impl ForkContract {
    pub(crate) fn validate(&self, activity_count: usize) -> Result<(), String> {
        if self.branches.len() < 2
            || self.max_concurrency == 0
            || self.max_concurrency as usize > self.branches.len()
        {
            return Err("fork requires at least two branches and bounded concurrency".into());
        }
        let mut activities = BTreeSet::new();
        for (name, branch) in &self.branches {
            if name.trim().is_empty() || branch.activity >= activity_count {
                return Err("fork branch name or body is invalid".into());
            }
            if !activities.insert(branch.activity) {
                return Err("fork branches require distinct body instances".into());
            }
        }
        Ok(())
    }

    /// Evaluate one immutable arrival snapshot. Repeated calls do not emit or
    /// persist a release; the run owner fences its release event exactly once.
    pub(crate) fn join(
        &self,
        identity: &CollectionIdentity,
        expected: &BTreeMap<String, BranchAttempt>,
        arrivals: &[BranchArrival],
    ) -> Result<JoinSnapshot, String> {
        if identity.run.is_empty() || identity.scope.is_empty() || identity.generation == 0 {
            return Err("join requires run/scope/generation identity".into());
        }
        if expected.keys().ne(self.branches.keys())
            || expected
                .values()
                .any(|owner| owner.activation.is_empty() || owner.attempt == 0)
            || expected
                .values()
                .map(|owner| &owner.activation)
                .collect::<BTreeSet<_>>()
                .len()
                != expected.len()
        {
            return Err("join requires exact declared branch activation/attempt owners".into());
        }
        let mut states = BTreeMap::new();
        for arrival in arrivals {
            if &arrival.identity != identity {
                return Err("join arrival has stale or foreign run/scope/generation".into());
            }
            if expected.get(&arrival.branch) != Some(&arrival.owner) {
                return Err("join arrival has stale or foreign branch activation/attempt".into());
            }
            if let Some(previous) = states.insert(arrival.branch.clone(), arrival.state)
                && previous != arrival.state
            {
                return Err("conflicting branch arrival".into());
            }
        }
        let state = if states.len() != self.branches.len() {
            AggregateState::Pending
        } else if self
            .branches
            .iter()
            .any(|(name, branch)| branch.required && states[name] != ArrivalState::Completed)
        {
            AggregateState::Blocked
        } else {
            AggregateState::Complete
        };
        Ok(JoinSnapshot {
            identity: identity.clone(),
            state,
            arrivals: states,
        })
    }
}
