//! Durable wake receipts, not external effects. Consumers key work by (key, attempt).
use super::{RunSnapshot, RunStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimerScope {
    Activation {
        execution_id: String,
        activation_id: String,
    },
    /// Independent services (such as proof cohorts) continue across run pauses.
    Service { owner: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimerPurpose {
    CohortFlush { cohort_id: String, rows: u16 },
    ProviderRetry { provider: String },
    QuestionDeadline { question_id: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimerOutcome {
    Wake,
    TimedOut,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimerState {
    Pending,
    Fired,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DurableTimer {
    /// Stable activation/effect identity, retained after delivery or cancellation.
    pub key: String,
    pub scope: TimerScope,
    pub purpose: TimerPurpose,
    pub due_ms: i64,
    pub expires_ms: i64,
    pub max_attempts: u16,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub expiry_outcome: TimerOutcome,
    pub attempt: u16,
    pub state: TimerState,
    pub outcome: Option<TimerOutcome>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimerCommand {
    Schedule {
        timer: DurableTimer,
    },
    Fire {
        key: String,
        attempt: u16,
    },
    Retry {
        key: String,
        attempt: u16,
        retry_after_ms: Option<u64>,
    },
    Cancel {
        key: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimerEvent {
    Scheduled {
        timer: DurableTimer,
    },
    Fired {
        key: String,
        attempt: u16,
        outcome: TimerOutcome,
    },
    Retried {
        key: String,
        attempt: u16,
        due_ms: i64,
        retry_after_ms: Option<u64>,
    },
    Cancelled {
        key: String,
    },
}

fn bounded_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

impl DurableTimer {
    fn validate(&self) -> Result<(), String> {
        let purpose_valid = match &self.purpose {
            TimerPurpose::CohortFlush { cohort_id, rows } => bounded_id(cohort_id) && *rows > 0,
            TimerPurpose::ProviderRetry { provider } => bounded_id(provider),
            TimerPurpose::QuestionDeadline { question_id } => {
                bounded_id(question_id)
                    && self.expiry_outcome != TimerOutcome::Wake
                    && self.due_ms == self.expires_ms
            }
        };
        if !bounded_id(&self.key)
            || !purpose_valid
            || self.due_ms < 0
            || self.expires_ms < self.due_ms
            || self.max_attempts == 0
            || self.max_attempts > 100
            || self.base_backoff_ms == 0
            || self.base_backoff_ms > self.max_backoff_ms
            || self.max_backoff_ms > 86_400_000
            || self.attempt != 1
            || self.state != TimerState::Pending
            || self.outcome.is_some()
        {
            return Err("invalid durable timer bounds or initial state".into());
        }
        if let TimerScope::Service { owner } = &self.scope
            && !bounded_id(owner)
        {
            return Err("invalid timer owner".into());
        }
        Ok(())
    }

    pub(super) fn eligible(&self, snapshot: &RunSnapshot) -> bool {
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            return false;
        }
        match &self.scope {
            TimerScope::Service { .. } => true,
            TimerScope::Activation {
                execution_id,
                activation_id,
            } => {
                snapshot.status == RunStatus::Running
                    && snapshot.graph_executions.iter().any(|g| {
                        g.id == *execution_id
                            && !g.completed
                            && g.activations.iter().any(|a| {
                                a.id == *activation_id
                                    && matches!(
                                        a.state,
                                        super::graph::ActivationState::Ready
                                            | super::graph::ActivationState::Running
                                    )
                            })
                    })
            }
        }
    }
}

fn timer<'a>(snapshot: &'a RunSnapshot, key: &str) -> Result<&'a DurableTimer, String> {
    snapshot
        .timers
        .iter()
        .find(|t| t.key == key)
        .ok_or_else(|| "timer not found".into())
}

fn retry_due(timer: &DurableTimer, at_ms: i64, hint: Option<u64>) -> i64 {
    let delay = timer
        .base_backoff_ms
        .saturating_mul(1u64 << u32::from(timer.attempt.saturating_sub(1).min(63)))
        .max(hint.unwrap_or(0))
        .min(timer.max_backoff_ms);
    at_ms.saturating_add(delay as i64).min(timer.expires_ms)
}

pub(super) fn choose(
    snapshot: &RunSnapshot,
    command: TimerCommand,
    at_ms: i64,
) -> Result<TimerEvent, String> {
    if !matches!(
        snapshot.event_contract_version,
        super::contracts::PINNED_SERIAL_EVENT_CONTRACT_VERSION
            | super::contracts::CURRENT_EVENT_CONTRACT_VERSION
    ) || matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled)
    {
        return Err("timer requires an active pinned-contract run".into());
    }
    match command {
        TimerCommand::Schedule { timer: new } => {
            new.validate()?;
            if snapshot.timers.len() >= 4096 || snapshot.timers.iter().any(|t| t.key == new.key) {
                return Err("timer key already reserved or timer budget exhausted".into());
            }
            if !new.eligible(snapshot) {
                return Err("timer owner is not active".into());
            }
            Ok(TimerEvent::Scheduled { timer: new })
        }
        TimerCommand::Fire { key, attempt } => {
            let t = timer(snapshot, &key)?;
            if t.state != TimerState::Pending
                || t.attempt != attempt
                || !t.eligible(snapshot)
                || at_ms < t.due_ms
            {
                return Err("timer is not due for this owner/attempt".into());
            }
            let outcome = if at_ms >= t.expires_ms || t.attempt >= t.max_attempts {
                t.expiry_outcome
            } else {
                TimerOutcome::Wake
            };
            Ok(TimerEvent::Fired {
                key,
                attempt,
                outcome,
            })
        }
        TimerCommand::Retry {
            key,
            attempt,
            retry_after_ms,
        } => {
            let t = timer(snapshot, &key)?;
            if t.state != TimerState::Fired
                || t.attempt != attempt
                || t.attempt >= t.max_attempts
                || at_ms >= t.expires_ms
                || !t.eligible(snapshot)
                || t.outcome != Some(TimerOutcome::Wake)
            {
                return Err("timer retry budget exhausted or owner unavailable".into());
            }
            Ok(TimerEvent::Retried {
                key,
                attempt: attempt + 1,
                due_ms: retry_due(t, at_ms, retry_after_ms),
                retry_after_ms,
            })
        }
        TimerCommand::Cancel { key } => {
            let t = timer(snapshot, &key)?;
            if t.state == TimerState::Cancelled {
                return Err("timer already cancelled".into());
            }
            Ok(TimerEvent::Cancelled { key })
        }
    }
}

/// Validate again during replay; serialized timer events cannot bypass budgets or consent fences.
pub(super) fn apply(
    snapshot: &mut RunSnapshot,
    event: &TimerEvent,
    at_ms: i64,
) -> Result<(), String> {
    let command = match event {
        TimerEvent::Scheduled { timer } => TimerCommand::Schedule {
            timer: timer.clone(),
        },
        TimerEvent::Fired { key, attempt, .. } => TimerCommand::Fire {
            key: key.clone(),
            attempt: *attempt,
        },
        TimerEvent::Retried {
            key,
            attempt,
            retry_after_ms,
            ..
        } => TimerCommand::Retry {
            key: key.clone(),
            attempt: attempt.checked_sub(1).ok_or("invalid timer attempt")?,
            retry_after_ms: *retry_after_ms,
        },
        TimerEvent::Cancelled { key } => TimerCommand::Cancel { key: key.clone() },
    };
    if choose(snapshot, command, at_ms)? != *event {
        return Err("invalid timer event".into());
    }
    match event {
        TimerEvent::Scheduled { timer } => snapshot.timers.push(timer.clone()),
        TimerEvent::Fired { key, outcome, .. } => {
            let t = snapshot
                .timers
                .iter_mut()
                .find(|t| t.key == *key)
                .ok_or("timer missing")?;
            t.state = TimerState::Fired;
            t.outcome = Some(*outcome);
        }
        TimerEvent::Retried {
            key,
            attempt,
            due_ms,
            ..
        } => {
            let t = snapshot
                .timers
                .iter_mut()
                .find(|t| t.key == *key)
                .ok_or("timer missing")?;
            t.state = TimerState::Pending;
            t.attempt = *attempt;
            t.due_ms = *due_ms;
            t.outcome = None;
        }
        TimerEvent::Cancelled { key } => {
            snapshot
                .timers
                .iter_mut()
                .find(|t| t.key == *key)
                .ok_or("timer missing")?
                .state = TimerState::Cancelled;
        }
    }
    Ok(())
}

pub(super) fn next_due(snapshot: &RunSnapshot) -> Option<i64> {
    snapshot
        .timers
        .iter()
        .filter(|t| t.state == TimerState::Pending && t.eligible(snapshot))
        .map(|t| t.due_ms)
        .min()
}

/// One bounded receipt per actor turn. No provider or tool is called by the clock.
pub(super) fn fire_due(
    store: &super::RunStore,
    snapshot: &RunSnapshot,
    at_ms: i64,
) -> Result<bool, String> {
    let Some(t) = snapshot
        .timers
        .iter()
        .find(|t| t.state == TimerState::Pending && t.eligible(snapshot) && t.due_ms <= at_ms)
    else {
        return Ok(false);
    };
    store.command_at(
        &snapshot.id,
        &format!(
            "timer:{}:fire:{}",
            hex::encode(Sha256::digest(t.key.as_bytes())),
            t.attempt
        ),
        super::RunCommand::Timer {
            command: TimerCommand::Fire {
                key: t.key.clone(),
                attempt: t.attempt,
            },
        },
        at_ms,
    )?;
    Ok(true)
}
