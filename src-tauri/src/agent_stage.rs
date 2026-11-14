//! Session stages are a projection of current facts, never mutable workflow state.
use crate::{
    AppState,
    agent_report::{AgentReport, ReportState, WaitFor},
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct AcceptedReport {
    report: AgentReport,
    at: u64,
    message_id: String,
    binding: Option<(String, Option<u64>)>,
}

#[derive(Default)]
pub(crate) struct Reports {
    pub(crate) health: Mutex<crate::agent_health::Tracker>,
    reports: Mutex<HashMap<String, AcceptedReport>>,
    failed_input: Mutex<HashMap<String, u64>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentReportView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<AgentReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_message_id: Option<String>,
    pub stage: Option<String>,
    pub stalled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_reason: Option<String>,
}

impl Reports {
    pub(crate) fn input_failed(&self, state: &AppState, pty: &str) {
        let epoch = state
            .session_maps
            .session_states
            .get(pty)
            .map(|s| s.turn_epoch)
            .unwrap_or(0);
        self.failed_input.lock().insert(pty.into(), epoch);
        crate::boss_attention::changed(state);
    }

    /// Mail insertion calls this once; reads, ACK and eviction do not change it.
    pub(crate) fn accept(&self, state: &AppState, message: &crate::state::AgentMessage) {
        let Some(report) = &message.report else {
            return;
        };
        let accepted = Self::prepare(state, &message.from_tuic_session, report.clone());
        self.commit(&message.from_tuic_session, &message.id, accepted);
        crate::boss_attention::changed(state);
    }

    pub(crate) fn prepare(state: &AppState, peer: &str, report: AgentReport) -> AcceptedReport {
        let binding = state.live_pty_for_peer(peer).map(|pty| {
            let epoch = state
                .session_maps
                .session_states
                .get(&pty)
                .map(|s| s.turn_epoch);
            (pty, epoch)
        });
        AcceptedReport {
            report,
            at: 0,
            message_id: String::new(),
            binding,
        }
    }

    pub(crate) fn commit(&self, peer: &str, message_id: &str, mut accepted: AcceptedReport) {
        // A report is activity. Rows and Boss attention re-read at once
        // (`boss_attention::changed`); a report comes from an agent tool call,
        // whose terminal output republishes the session state.
        if let Some((pty, _)) = &accepted.binding {
            self.health.lock().restart(pty);
        }
        let mut reports = self.reports.lock();
        accepted.at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        accepted.message_id = message_id.into();
        reports.insert(peer.into(), accepted);
    }

    pub(crate) fn view(
        &self,
        state: &AppState,
        pty: &str,
        peer: Option<&str>,
        snapshot: Option<&crate::state::SessionState>,
    ) -> AgentReportView {
        let mut view = AgentReportView::default();
        if let Some(accepted) = peer.and_then(|peer| self.reports.lock().get(peer).cloned()) {
            // Match both the live assignment (PTY) and its submitted-input epoch.
            // An absent initial accumulator is epoch zero, not a timestamp guess.
            view.report_at = Some(accepted.at);
            view.report_message_id = Some(accepted.message_id.clone());
            let epoch = snapshot.map(|s| s.turn_epoch).unwrap_or(0);
            if accepted
                .binding
                .as_ref()
                .is_some_and(|(id, turn)| id == pty && turn.unwrap_or(0) == epoch)
            {
                view.report = Some(accepted.report);
                view.report_at = Some(accepted.at);
                view.report_message_id = Some(accepted.message_id);
            }
        }
        if !state.session_maps.sessions.contains_key(pty)
            || state.session_maps.exit_codes.contains_key(pty)
            || peer.is_none_or(|peer| state.live_pty_for_peer(peer).as_deref() != Some(pty))
        {
            return view;
        }
        let report = view.report.as_ref();
        let wait = report.and_then(|r| r.wait_for);
        let report_state = report.map(|r| r.state);
        // Ordered rule table: first match wins. progress is intentionally absent.
        let failed_input = self.failed_input.lock().get(pty).copied()
            == Some(snapshot.map(|s| s.turn_epoch).unwrap_or(0));
        let uncertain_input = state
            .session_maps
            .silence_states
            .get(pty)
            .is_some_and(|s| s.lock().injection_delivery_uncertain);
        let rules = [
            (
                failed_input || uncertain_input,
                "undelivered_input",
                "input_write_failed_or_uncertain",
            ),
            (
                state.pending_initial_prompts.contains_key(pty),
                "undelivered_input",
                "initial_prompt_pending",
            ),
            (
                wait == Some(WaitFor::Boss),
                "waiting_on_boss",
                "report_waiting_boss",
            ),
            (
                snapshot.is_some_and(|s| s.awaiting_input || s.choice_prompt.is_some()),
                "waiting_on_boss",
                "native_question",
            ),
            (
                wait == Some(WaitFor::Review) || report_state == Some(ReportState::Ready),
                "waiting_on_review",
                "report_ready_or_review",
            ),
            (
                wait == Some(WaitFor::Dependency)
                    || matches!(
                        report_state,
                        Some(ReportState::Blocked | ReportState::Failed | ReportState::Dropped)
                    ),
                "blocked",
                "report_blocked",
            ),
            (
                snapshot.is_some_and(|s| s.queued_commands > 0),
                "queued",
                "commands_queued",
            ),
            (
                snapshot.is_some_and(|s| {
                    !s.background_work
                        && matches!(s.agent_state.as_deref(), Some("idle" | "completed"))
                }),
                "idle",
                "agent_idle",
            ),
            (
                snapshot.is_some_and(|s| {
                    s.background_work || s.agent_state.as_deref() == Some("working")
                }),
                "working",
                "agent_working",
            ),
        ];
        if let Some((_, stage, reason)) = rules.into_iter().find(|(matches, _, _)| *matches) {
            view.stage = Some(stage.into());
            view.stage_reason = Some(reason.into());
        }
        // One silence rule for every session (story 3050): the snapshot carries
        // the stall; it overrides a working or queued stage, never a wait.
        if crate::agent_health::eligible(&view) && snapshot.is_some_and(|s| s.stalled) {
            view.stage = Some("stalled".into());
            view.stage_reason = Some("silent_past_threshold".into());
            view.stalled = true;
        }
        view
    }
}

impl AgentReportView {
    pub(crate) fn extend(&self, value: &mut serde_json::Value) {
        if let (Some(target), Ok(serde_json::Value::Object(fields))) =
            (value.as_object_mut(), serde_json::to_value(self))
        {
            target.extend(fields);
        }
    }
}
