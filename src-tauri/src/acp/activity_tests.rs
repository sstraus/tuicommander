//! Activity derived from the native ego 0.4.1 recording, frame by frame.

use std::collections::HashMap;

use agent_client_protocol::schema::v1;
use serde_json::{Value, json};

use super::*;

const RECORDING: &str =
    include_str!("../../tests/fixtures/acp/ego-child-activity-native-0.4.1.jsonl");

const CANCELLED_SESSION: &str = "2ea6b97c-e72f-5fec-96a7-0c8677710aaa";

fn messages() -> Vec<(String, Value)> {
    RECORDING
        .lines()
        .map(|line| {
            let mut entry: Value = serde_json::from_str(line).expect("recorded line is JSON");
            let direction = entry["direction"].as_str().expect("direction").to_owned();
            (direction, entry["message"].take())
        })
        .collect()
}

/// The update recorded at `index`, as the actor receives it.
fn update(index: usize) -> v1::SessionUpdate {
    let (_, message) = &messages()[index];
    assert_eq!(message["method"], "session/update", "line {index}");
    serde_json::from_value::<v1::SessionNotification>(message["params"].clone())
        .expect("recorded notification parses")
        .update
}

#[derive(Default)]
struct Session {
    tracker: ActivityTracker,
    running: bool,
    activity: AcpChatActivity,
    /// Every state this session published, in order.
    states: Vec<AcpChatActivityState>,
}

impl Session {
    fn refresh(&mut self) {
        let next = self
            .tracker
            .derive(self.running, Vec::new(), &self.activity);
        if next.revision != self.activity.revision {
            self.states.push(next.state);
        }
        self.activity = next;
    }
}

/// Run the whole recording through one tracker per session the way the
/// connection actor does: a prompt starts a turn, its answer ends it.
fn replay() -> HashMap<String, Session> {
    let mut sessions: HashMap<String, Session> = HashMap::new();
    let mut prompts: HashMap<u64, String> = HashMap::new();
    for (direction, message) in messages() {
        let method = message["method"].as_str();
        match (direction.as_str(), method) {
            ("client_to_ego", Some("session/prompt")) => {
                let session_id = message["params"]["sessionId"].as_str().unwrap().to_owned();
                prompts.insert(message["id"].as_u64().unwrap(), session_id.clone());
                let session = sessions.entry(session_id).or_default();
                session.running = true;
                session.tracker.turn_started();
                session.refresh();
            }
            ("ego_to_client", Some("session/update")) => {
                let notification: v1::SessionNotification =
                    serde_json::from_value(message["params"].clone()).unwrap();
                let session = sessions
                    .entry(notification.session_id.0.to_string())
                    .or_default();
                if session
                    .tracker
                    .observe(&notification.update, session.running, true)
                {
                    session.refresh();
                }
            }
            ("ego_to_client", None) => {
                let Some(session_id) = message["id"].as_u64().and_then(|id| prompts.remove(&id))
                else {
                    continue;
                };
                let session = sessions.get_mut(&session_id).unwrap();
                session.running = false;
                session
                    .tracker
                    .turn_ended(message["result"]["stopReason"] == "cancelled");
                session.refresh();
            }
            _ => {}
        }
    }
    sessions
}

fn durable(turn: &str, item: &str) -> Value {
    json!({
        "turnId": turn,
        "itemId": item,
        "child": {
            "kind": "durable",
            "invocation": "01a19689-0000-7000-8000-000000000001",
            "run": {
                "session_id": "01a19689-0000-7000-8000-000000000002",
                "nonce": "01a19689-0000-7000-8000-000000000003"
            }
        }
    })
}

fn info(frame: Value) -> v1::SessionUpdate {
    serde_json::from_value(json!({
        "sessionUpdate": "session_info_update",
        "_meta": { "ego": { "childActivity": frame } }
    }))
    .unwrap()
}

fn lifecycle(entry: &Value, phase: Value) -> v1::SessionUpdate {
    let mut frame = entry.clone();
    frame["version"] = json!(1);
    frame["kind"] = json!("lifecycle");
    frame["phase"] = phase;
    info(frame)
}

fn idle() -> AcpChatActivity {
    AcpChatActivity::default()
}

// catches: a settled lifecycle or the turn's own end not retracting a child
// or tool, so a recorded chat stays busy after ego went quiet.
#[test]
fn every_recorded_session_ends_idle_with_nothing_live() {
    for (session_id, session) in replay() {
        assert_eq!(
            session.activity.state,
            AcpChatActivityState::Idle,
            "{session_id}"
        );
        assert!(session.activity.tools.is_empty(), "{session_id}");
        assert!(session.activity.children.is_empty(), "{session_id}");
    }
}

// catches: tool and child frames being dropped, so a recorded turn that ran a
// tool never reads busy beyond the bare prompt.
#[test]
fn recorded_child_and_tool_are_live_while_their_turn_runs() {
    let mut tracker = ActivityTracker::default();
    tracker.turn_started();
    assert!(tracker.observe(&update(17), true, true), "tool_call opens");
    assert!(tracker.observe(&update(20), true, true), "child started");
    let live = tracker.derive(false, Vec::new(), &idle());
    assert_eq!(live.state, AcpChatActivityState::Busy);
    assert_eq!(live.tools.len(), 1);
    assert_eq!(live.children.len(), 1);
    assert_eq!(
        live.children[0].turn_id,
        "01a19689-1baf-7077-af31-86a014d6656d"
    );

    assert!(tracker.observe(&update(22), true, true), "child settled");
    assert!(tracker.observe(&update(23), true, true), "tool completed");
    let quiet = tracker.derive(false, Vec::new(), &live);
    assert_eq!(quiet.state, AcpChatActivityState::Idle);
    assert_eq!(quiet.revision, live.revision + 1);
}

// catches: the cancelled stop reason not reaching the activity, so a stopped
// chat cannot be told from one that finished.
#[test]
fn recorded_cancel_leaves_only_that_session_interrupted() {
    for (session_id, session) in replay() {
        assert_eq!(
            session.activity.interrupted,
            session_id == CANCELLED_SESSION,
            "{session_id}"
        );
    }
}

// catches: the recorded refused permission (an error answer) marking the
// turn interrupted, which only a cancel means.
#[test]
fn a_failed_turn_is_not_interrupted() {
    let mut tracker = ActivityTracker::default();
    tracker.turn_started();
    tracker.observe(&update(71), true, true);
    tracker.turn_ended(false);
    let activity = tracker.derive(false, Vec::new(), &idle());
    assert!(!activity.interrupted);
    assert_eq!(activity.tools, Vec::new());
}

// catches: a `session/load` replay of an `in_progress` tool call latching
// busy with no turn behind it.
#[test]
fn replayed_tool_call_without_a_turn_does_not_latch_busy() {
    let mut tracker = ActivityTracker::default();
    assert!(!tracker.observe(&update(106), false, true));
    assert!(!tracker.observe(&update(107), false, true));
    assert_eq!(tracker.derive(false, Vec::new(), &idle()), idle());
}

// catches: a lifecycle that arrives after its foreground turn was cleared
// bringing the child back for good.
#[test]
fn late_lifecycle_after_a_foreground_clear_is_ignored() {
    let mut tracker = ActivityTracker::default();
    tracker.observe(&update(20), true, true);
    assert!(
        tracker.observe(&update(27), true, true),
        "clear removes the child"
    );
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
    assert!(
        !tracker.observe(&update(20), true, true),
        "late start ignored"
    );
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
}

// catches: ending the turn or a foreground clear removing a durable child,
// which outlives the turn that started it by design.
#[test]
fn durable_child_survives_its_turn_and_a_foreground_clear() {
    let turn = "01a19689-1baf-7077-af31-86a014d6656d";
    let entry = durable(turn, "01a19689-1bf0-7072-9288-000000000009");
    let mut tracker = ActivityTracker::default();
    tracker.turn_started();
    assert!(tracker.observe(&lifecycle(&entry, json!({"phase": "started"})), true, true));
    assert!(
        tracker.observe(&update(20), true, true),
        "foreground sibling"
    );
    tracker.turn_ended(false);
    tracker.observe(&update(27), false, true);
    let after = tracker.derive(false, Vec::new(), &idle());
    assert_eq!(
        after.state,
        AcpChatActivityState::Busy,
        "durable keeps it busy"
    );
    assert_eq!(after.children.len(), 1);
    assert_eq!(after.children[0].child["kind"], "durable");

    assert!(tracker.observe(
        &lifecycle(&entry, json!({"phase": "settled", "outcome": "completed"})),
        false,
        true
    ));
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
}

// catches: a session-wide clear (`turnId: null`) leaving durable children.
#[test]
fn recorded_session_clear_removes_every_child() {
    let entry = durable(
        "01a19689-1baf-7077-af31-86a014d6656d",
        "01a19689-0000-7000-8000-00000000000a",
    );
    let mut tracker = ActivityTracker::default();
    tracker.observe(&lifecycle(&entry, json!({"phase": "started"})), false, true);
    assert!(tracker.observe(&update(114), false, true));
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
}

// catches: an empty snapshot treated as "no news" instead of replacing the
// roster, so a reconnect keeps children ego no longer runs.
#[test]
fn recorded_empty_snapshot_replaces_the_roster() {
    let mut tracker = ActivityTracker::default();
    tracker.observe(&update(93), true, true);
    assert!(tracker.observe(&update(111), true, true));
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
}

// catches: frames read without the negotiated opt-in, or of a version this
// client was not written against.
#[test]
fn unnegotiated_or_unknown_version_frames_change_nothing() {
    let mut tracker = ActivityTracker::default();
    assert!(!tracker.observe(&update(20), true, false));
    let mut entry = durable("t", "i");
    entry["version"] = json!(2);
    entry["kind"] = json!("lifecycle");
    entry["phase"] = json!({"phase": "started"});
    assert!(!tracker.observe(&info(entry), true, true));
    assert_eq!(
        tracker.derive(false, Vec::new(), &idle()).children,
        Vec::new()
    );
}

// catches: a revision that moves on every publish (a host would then never
// prefer its snapshot) or not at all (a stale snapshot would win).
#[test]
fn revision_grows_only_when_the_meaning_changes() {
    let tracker = ActivityTracker::default();
    let busy = tracker.derive(true, Vec::new(), &idle());
    assert_eq!(busy.revision, 1);
    assert_eq!(tracker.derive(true, Vec::new(), &busy).revision, 1);
    assert_eq!(tracker.derive(false, Vec::new(), &busy).revision, 2);
}

// catches: a running turn hiding a person's wait behind "busy".
#[test]
fn a_human_wait_outranks_a_running_turn() {
    let tracker = ActivityTracker::default();
    let wait = AcpHumanWait {
        request_id: AcpHostRequestId::new(),
        kind: AcpHumanWaitKind::Question,
    };
    let awaiting = tracker.derive(true, vec![wait.clone()], &idle());
    assert_eq!(awaiting.state, AcpChatActivityState::Awaiting);
    assert_eq!(awaiting.human_waits, vec![wait]);
}

// catches: a connection that ended still publishing busy or awaiting, which
// no later frame can ever retract.
#[test]
fn a_retired_activity_is_idle_and_marks_a_running_turn_interrupted() {
    let mut tracker = ActivityTracker::default();
    tracker.observe(&update(20), true, true);
    let wait = AcpHumanWait {
        request_id: AcpHostRequestId::new(),
        kind: AcpHumanWaitKind::Permission,
    };
    let live = tracker.derive(true, vec![wait], &idle());
    let retired = live.retired(true);
    assert_eq!(retired.state, AcpChatActivityState::Idle);
    assert!(retired.human_waits.is_empty() && retired.children.is_empty());
    assert!(retired.interrupted);
    assert_eq!(retired.revision, live.revision + 1);
    assert_eq!(
        idle().retired(false).revision,
        0,
        "nothing changed, no revision"
    );
}
