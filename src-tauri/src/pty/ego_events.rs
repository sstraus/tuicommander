//! The open native modal of an ego terminal, from its `agent-event` frames.
//!
//! ego 0.4.3 (story 771-b309) writes `OSC 7770;agent-event=<base64 JSON>`. Its
//! coarse `state=` frames stay the authority for busy, idle and awaiting; this
//! owner only remembers which native modal is open, so its question payload
//! can name the wait that coarse state already holds. It never changes state.
//!
//! Open: `modal_open` of the current native session. Settle: `modal_end` of
//! the same id, `turn_end` of its turn, `session_start` / `session_end`. A
//! settled id never opens again.
//!
//! Tool readout: `tool_start` of the current native session starts its item
//! with the bounded `data.label`; `tool_end` of the same id ends it; `turn_end`,
//! `session_start` and `session_end` end all.
//!
//! Turn readout (#2674-1be8): `turn_start` opens the turn and forgets the last
//! outcome; `child_start` / `child_activity` / `child_end` of that turn add,
//! relabel and remove a child by id; `turn_end` records `data.outcome` and ends
//! the turn and its children. Frames of another or a settled turn change
//! nothing. Frames before `session_start` are ignored: the contract names it
//! the public header of every stream and describes no mid-stream attach.

use base64::Engine;
use serde::Deserialize;
use std::collections::VecDeque;

/// The decoded frame is at most 16 KiB (ego `terminal-agent-events.md`).
const MAX_FRAME_BYTES: usize = 16 * 1024;
/// Children kept in flight at most; a duplicate or extra start changes nothing.
const MAX_CHILDREN: usize = 64;

#[derive(Deserialize)]
struct Frame {
    version: u32,
    agent: String,
    session: String,
    turn: Option<String>,
    event: String,
    id: Option<String>,
    #[serde(default)]
    data: serde_json::Value,
}

struct Modal {
    id: String,
    turn: Option<String>,
    card: Option<crate::agent_hook_payload::QuestionCard>,
}

#[derive(Default)]
pub(super) struct EgoEvents {
    session: Option<String>,
    modal: Option<Modal>,
    settled: VecDeque<String>,
    pub(super) tools: crate::agent_tool_readout::ToolReadout,
    turn: Option<String>,
    /// `(id, label)` of the children in flight, in start order.
    children: Vec<(String, String)>,
    outcome: Option<String>,
    stop_generation: u64,
}

/// A display label: control characters dropped, at most the tool label length.
fn child_label(data: &serde_json::Value) -> String {
    data.get("label")
        .and_then(|label| label.as_str())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(crate::agent_tool_readout::MAX_LABEL_CHARS)
        .collect()
}

impl EgoEvents {
    pub(super) fn stop_generation(&self) -> u64 {
        self.stop_generation
    }

    /// The native id of the open modal, if any.
    pub(super) fn wait_request(&self) -> Option<&str> {
        self.modal.as_ref().map(|modal| modal.id.as_str())
    }

    pub(super) fn wait_card(&self) -> Option<&crate::agent_hook_payload::QuestionCard> {
        self.modal.as_ref().and_then(|modal| modal.card.as_ref())
    }

    /// Labels of the children in flight, in start order.
    pub(super) fn children(&self) -> Vec<String> {
        self.children
            .iter()
            .map(|(_, label)| label.clone())
            .collect()
    }

    /// Outcome of the last settled turn.
    pub(super) fn outcome(&self) -> Option<&str> {
        self.outcome.as_deref()
    }

    /// Forget the turn, its children and the last outcome.
    fn end_session(&mut self) {
        self.turn = None;
        self.children.clear();
        self.outcome = None;
    }

    /// Apply a child frame of the current turn.
    fn child(&mut self, frame: &Frame) {
        let Some(id) = frame.id.as_deref().filter(|id| !id.is_empty()) else {
            return;
        };
        if self.turn.is_none() || frame.turn != self.turn {
            return;
        }
        let held = self.children.iter().position(|(held, _)| held == id);
        match (frame.event.as_str(), held) {
            ("child_start", None) if self.children.len() < MAX_CHILDREN => {
                self.children
                    .push((id.to_owned(), child_label(&frame.data)));
            }
            ("child_activity", Some(index)) => self.children[index].1 = child_label(&frame.data),
            ("child_end", Some(index)) => {
                self.children.remove(index);
            }
            _ => {}
        }
    }

    fn settle(&mut self) {
        if let Some(modal) = self.modal.take() {
            if self.settled.len() == 64 {
                self.settled.pop_front();
            }
            self.settled.push_back(modal.id);
        }
    }

    /// Apply one frame payload. Unknown versions and events change nothing.
    pub(super) fn receive(&mut self, payload: &str) {
        if payload.len() > MAX_FRAME_BYTES * 2 {
            return;
        }
        let Some(frame) = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok()
            .filter(|bytes| bytes.len() <= MAX_FRAME_BYTES)
            .and_then(|bytes| serde_json::from_slice::<Frame>(&bytes).ok())
        else {
            return;
        };
        if frame.version != 1 || frame.agent != "ego" || frame.session.is_empty() {
            return;
        }
        if frame.event == "session_start" {
            self.settle();
            self.tools.clear();
            self.end_session();
            self.session = Some(frame.session);
            return;
        }
        if self.session.as_deref() != Some(frame.session.as_str()) {
            return;
        }
        match frame.event.as_str() {
            "turn_start" if frame.turn.is_some() => {
                self.children.clear();
                self.outcome = None;
                self.turn = frame.turn;
            }
            "child_start" | "child_activity" | "child_end" => self.child(&frame),
            "tool_start" => {
                if let (Some(id), Some(label)) = (
                    frame.id.as_deref(),
                    frame.data.get("label").and_then(|label| label.as_str()),
                ) {
                    self.tools.start(id, label);
                }
            }
            "tool_end" => {
                if let Some(id) = frame.id.as_deref() {
                    self.tools.end(id);
                }
            }
            "modal_open" => {
                let Some(id) = frame.id.filter(|id| !id.is_empty() && id.len() <= 256) else {
                    return;
                };
                if self.settled.contains(&id) || self.wait_request() == Some(id.as_str()) {
                    return;
                }
                self.settle();
                self.modal = Some(Modal {
                    card: crate::agent_hook_payload::ego_card(&id, &frame.data),
                    id,
                    turn: frame.turn,
                });
            }
            "modal_end" if frame.id.is_some() && frame.id.as_deref() == self.wait_request() => {
                self.settle();
            }
            "turn_end" => {
                self.tools.clear();
                if self.turn.is_some() && frame.turn == self.turn {
                    self.outcome = frame
                        .data
                        .get("outcome")
                        .and_then(|outcome| outcome.as_str())
                        .map(|outcome| {
                            outcome
                                .chars()
                                .take(crate::agent_tool_readout::MAX_LABEL_CHARS)
                                .collect()
                        });
                    if self.outcome.as_deref() == Some("interrupted") {
                        self.stop_generation = self.stop_generation.wrapping_add(1);
                    }
                    self.turn = None;
                    self.children.clear();
                }
                if self
                    .modal
                    .as_ref()
                    .is_some_and(|modal| modal.turn.is_none() || modal.turn == frame.turn)
                {
                    self.settle();
                }
            }
            "session_end" => {
                self.settle();
                self.tools.clear();
                self.end_session();
                self.session = None;
            }
            _ => {}
        }
    }

    pub(super) fn clear(&mut self) {
        self.settle();
        self.tools.clear();
        self.end_session();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde_json::json;

    fn frame(
        session: &str,
        turn: Option<&str>,
        event: &str,
        id: Option<&str>,
        data: serde_json::Value,
    ) -> String {
        base64::engine::general_purpose::STANDARD.encode(
            json!({"version": 1, "agent": "ego", "session": session, "turn": turn,
                   "event": event, "id": id, "data": data})
            .to_string(),
        )
    }

    fn question() -> serde_json::Value {
        json!({"kind": "question", "question": {"prompt": "Pick", "mode": "single",
               "options": [{"id": "a", "label": "A"}], "free_text": false}})
    }

    fn opened() -> EgoEvents {
        let mut events = EgoEvents::default();
        events.receive(&frame("s1", None, "session_start", None, json!({})));
        events.receive(&frame(
            "s1",
            Some("t1"),
            "modal_open",
            Some("m1"),
            question(),
        ));
        assert_eq!(events.wait_request(), Some("m1"));
        assert!(events.wait_card().is_some());
        events
    }

    // Catches: a rejected or cancelled modal (no answer) keeps its wait, or a
    // modal_end of another id settles the open one.
    #[test]
    fn modal_end_settles_only_its_own_id() {
        let mut events = opened();
        events.receive(&frame(
            "s1",
            Some("t1"),
            "modal_end",
            Some("other"),
            json!({"outcome": "answered"}),
        ));
        assert_eq!(events.wait_request(), Some("m1"));
        events.receive(&frame(
            "s1",
            Some("t1"),
            "modal_end",
            Some("m1"),
            json!({"outcome": "rejected"}),
        ));
        assert_eq!(events.wait_request(), None);
    }

    // Catches: a stopped turn or an exited session leaves the modal open; a
    // late modal_open of the settled id reopens it.
    #[test]
    fn turn_end_session_end_and_exit_settle() {
        for end in ["turn_end", "session_end", "exit"] {
            let mut events = opened();
            match end {
                "exit" => events.clear(),
                _ => events.receive(&frame(
                    "s1",
                    Some("t1"),
                    end,
                    None,
                    json!({"outcome": "interrupted"}),
                )),
            }
            assert_eq!(events.wait_request(), None, "{end}");
            events.receive(&frame("s1", None, "session_start", None, json!({})));
            events.receive(&frame(
                "s1",
                Some("t1"),
                "modal_open",
                Some("m1"),
                question(),
            ));
            assert_eq!(events.wait_request(), None, "late modal_open after {end}");
        }
    }

    // Catches: a frame of another native session, version or agent, or an
    // over-cap / malformed frame, opens or settles the wait.
    #[test]
    fn foreign_and_malformed_frames_change_nothing() {
        let mut events = opened();
        events.receive(&frame("s2", Some("t1"), "turn_end", None, json!({})));
        events.receive("not base64!");
        events.receive(&base64::engine::general_purpose::STANDARD.encode("{}"));
        let big = "x".repeat(MAX_FRAME_BYTES);
        events.receive(&frame(
            "s1",
            Some("t1"),
            "modal_end",
            Some("m1"),
            json!({"pad": big}),
        ));
        let other = base64::engine::general_purpose::STANDARD.encode(
            json!({"version": 2, "agent": "ego", "session": "s1", "turn": "t1",
                   "event": "modal_end", "id": "m1"})
            .to_string(),
        );
        events.receive(&other);
        assert_eq!(events.wait_request(), Some("m1"));

        let mut fresh = EgoEvents::default();
        fresh.receive(&frame(
            "s1",
            Some("t1"),
            "modal_open",
            Some("m1"),
            question(),
        ));
        assert_eq!(
            fresh.wait_request(),
            None,
            "no modal before the native session starts"
        );
    }

    fn recorded(name: &str) -> Vec<serde_json::Value> {
        let path = format!(
            "{}/src/fixtures/agent_tool_readout/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The readout after each recorded frame.
    fn readout_trace(frames: &[serde_json::Value]) -> Vec<Option<String>> {
        let mut events = EgoEvents::default();
        frames
            .iter()
            .map(|frame| {
                events
                    .receive(&base64::engine::general_purpose::STANDARD.encode(frame.to_string()));
                events.tools.current().map(str::to_owned)
            })
            .collect()
    }

    // Catches: the recorded ego 0.4.3 tool_start label is not read, its
    // completed / failed tool_end leaves the readout, or a cancelled turn
    // with no tool_end keeps it.
    #[test]
    fn recorded_ego_tool_frames_start_and_end_the_readout() {
        let read = Some("read".to_owned());
        assert_eq!(
            readout_trace(&recorded("ego-0.4.3-tool.json")),
            vec![None, None, read, None, None, None]
        );

        let cancel = recorded("ego-0.4.3-cancel.json");
        let trace = readout_trace(&cancel);
        let events: Vec<&str> = cancel
            .iter()
            .map(|f| f["event"].as_str().unwrap())
            .collect();
        let end = events.iter().position(|e| *e == "tool_end").unwrap();
        assert!(
            trace[2..end]
                .iter()
                .all(|tool| tool.as_deref() == Some("task"))
        );
        assert_eq!(trace[end], None, "failed tool_end");

        let no_end: Vec<_> = cancel
            .iter()
            .filter(|frame| frame["event"] != "tool_end")
            .cloned()
            .collect();
        let trace = readout_trace(&no_end);
        let turn_end = no_end
            .iter()
            .position(|f| f["event"] == "turn_end")
            .unwrap();
        assert_eq!(trace[turn_end - 1].as_deref(), Some("task"));
        assert_eq!(trace[turn_end], None, "cancelled turn_end");
    }

    // Catches: a tool frame of another native session, or a tool_end of
    // another id, changes the readout; a tool end with the modal's id closes
    // the open modal; a modal ending clears a live tool.
    #[test]
    fn tool_end_and_modal_end_touch_only_their_own_item() {
        let mut events = opened();
        events.receive(&frame(
            "s1",
            Some("t1"),
            "tool_start",
            Some("c1"),
            json!({"label": "exec"}),
        ));
        events.receive(&frame(
            "s2",
            Some("t1"),
            "tool_start",
            Some("c2"),
            json!({"label": "read"}),
        ));
        events.receive(&frame("s1", Some("t1"), "tool_end", Some("c9"), json!({})));
        events.receive(&frame("s1", Some("t1"), "tool_end", Some("m1"), json!({})));
        assert_eq!(
            events.wait_request(),
            Some("m1"),
            "a tool end closed the modal"
        );
        events.receive(&frame("s1", Some("t1"), "modal_end", Some("m1"), json!({})));
        assert_eq!(events.tools.current(), Some("exec"));
        events.receive(&frame("s1", Some("t1"), "tool_end", Some("c1"), json!({})));
        assert_eq!(events.tools.current(), None);
    }

    // Catches: a permission modal is dropped (it is a native wait too) or
    // gains a question card.
    #[test]
    fn permission_modal_has_a_request_but_no_card() {
        let mut events = EgoEvents::default();
        events.receive(&frame("s1", None, "session_start", None, json!({})));
        events.receive(&frame(
            "s1",
            Some("t1"),
            "modal_open",
            Some("p1"),
            json!({"kind": "permission", "label": "exec"}),
        ));
        assert_eq!(events.wait_request(), Some("p1"));
        assert!(events.wait_card().is_none());
    }
}
