//! Event-to-suggestion bridge.
//!
//! `TriggerClassifier` evaluates `CommandOutcome` records and emits
//! `Suggestion` payloads when a terminal event looks worth investigating.
//! A per-session debounce prevents notification fatigue.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::knowledge::{CommandOutcome, OutcomeClass};

const DEBOUNCE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Suggestion {
    pub session_id: String,
    pub trigger_reason: String,
    pub proposed_goal: String,
}

pub(crate) struct TriggerClassifier {
    last_suggestion: parking_lot::Mutex<HashMap<String, Instant>>,
}

impl TriggerClassifier {
    pub fn new() -> Self {
        Self {
            last_suggestion: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub fn evaluate(&self, session_id: &str, outcome: &CommandOutcome) -> Option<Suggestion> {
        match &outcome.classification {
            OutcomeClass::Success
            | OutcomeClass::TuiLaunched { .. }
            | OutcomeClass::UserCancelled
            | OutcomeClass::Inferred => return None,
            OutcomeClass::Error { .. } | OutcomeClass::Timeout => {}
        }

        let mut guard = self.last_suggestion.lock();
        if let Some(last) = guard.get(session_id)
            && last.elapsed() < DEBOUNCE
        {
            return None;
        }
        guard.insert(session_id.to_string(), Instant::now());
        drop(guard);

        let (trigger_reason, proposed_goal) = match &outcome.classification {
            OutcomeClass::Timeout => (
                format!("Command timed out: {}", outcome.command),
                format!(
                    "Investigate why `{}` timed out after {}ms",
                    outcome.command, outcome.duration_ms
                ),
            ),
            OutcomeClass::Error { error_type } => {
                let exit_str = outcome
                    .exit_code
                    .map(|c| format!(" with exit code {c}"))
                    .unwrap_or_default();
                (
                    format!("Command failed: {} ({error_type})", outcome.command),
                    format!("Investigate why `{}` failed{exit_str}", outcome.command),
                )
            }
            _ => unreachable!(),
        };

        Some(Suggestion {
            session_id: session_id.to_string(),
            trigger_reason,
            proposed_goal,
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_outcome(class: OutcomeClass, cmd: &str, exit_code: Option<i32>) -> CommandOutcome {
        CommandOutcome {
            timestamp: 0,
            command: cmd.to_string(),
            cwd: "/tmp".to_string(),
            exit_code,
            output_snippet: String::new(),
            classification: class,
            duration_ms: 1000,
            id: 0,
        }
    }

    #[test]
    fn success_returns_none() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(OutcomeClass::Success, "ls", Some(0));
        assert!(tc.evaluate("s1", &outcome).is_none());
    }

    #[test]
    fn error_returns_suggestion() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(
            OutcomeClass::Error {
                error_type: "compile".to_string(),
            },
            "cargo build",
            Some(1),
        );
        let s = tc.evaluate("s1", &outcome).unwrap();
        assert!(s.trigger_reason.contains("cargo build"));
        assert!(s.proposed_goal.contains("exit code 1"));
        assert_eq!(s.session_id, "s1");
    }

    #[test]
    fn timeout_returns_suggestion() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(OutcomeClass::Timeout, "slow-cmd", None);
        let s = tc.evaluate("s1", &outcome).unwrap();
        assert!(s.trigger_reason.contains("timed out"));
    }

    #[test]
    fn debounce_suppresses_second() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(
            OutcomeClass::Error {
                error_type: "test".to_string(),
            },
            "pytest",
            Some(1),
        );
        assert!(tc.evaluate("s1", &outcome).is_some());
        assert!(tc.evaluate("s1", &outcome).is_none());
    }

    #[test]
    fn debounce_is_per_session() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(
            OutcomeClass::Error {
                error_type: "test".to_string(),
            },
            "pytest",
            Some(1),
        );
        assert!(tc.evaluate("s1", &outcome).is_some());
        assert!(tc.evaluate("s2", &outcome).is_some());
    }

    #[test]
    fn user_cancelled_returns_none() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(OutcomeClass::UserCancelled, "sleep 999", None);
        assert!(tc.evaluate("s1", &outcome).is_none());
    }

    #[test]
    fn inferred_returns_none() {
        let tc = TriggerClassifier::new();
        let outcome = make_outcome(OutcomeClass::Inferred, "unknown", None);
        assert!(tc.evaluate("s1", &outcome).is_none());
    }
}
