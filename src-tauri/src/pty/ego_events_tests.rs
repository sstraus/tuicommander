// Native ego `agent-event` frames: replay of real 0.4.3 captures through the
// chunk pipeline, and the turn readout's isolation rules. Included into
// `pty::tests`.

fn ego_frame(
    session: &str,
    turn: Option<&str>,
    event: &str,
    id: Option<&str>,
    data: serde_json::Value,
) -> String {
    use base64::Engine;
    let json = serde_json::json!({
        "version": 1, "agent": "ego", "session": session,
        "turn": turn, "event": event, "id": id, "data": data,
    });
    base64::engine::general_purpose::STANDARD.encode(json.to_string())
}

/// Published turn readout events of one replay, in order.
struct EgoReplay {
    children: Vec<Vec<String>>,
    outcomes: Vec<Option<String>>,
    questions: usize,
}

/// One read per OSC frame, as ego writes them; `chunk_chars` re-splits each read.
fn ego_reads(stream: &str, chunk_chars: Option<usize>) -> Vec<String> {
    let mut reads = Vec::new();
    let mut rest = stream;
    while let Some(end) = rest.find(['\x07', '\\']) {
        reads.push(rest[..=end].to_owned());
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        reads.push(rest.to_owned());
    }
    match chunk_chars {
        None => reads,
        Some(size) => {
            let chars: Vec<char> = stream.chars().collect();
            chars
                .chunks(size)
                .map(|piece| piece.iter().collect())
                .collect()
        }
    }
}

fn replay_ego_reads(agent: Option<&str>, reads: &[String]) -> EgoReplay {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "ego-native";
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = agent.map(str::to_string);
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 2000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    let mut events = state.event_bus.subscribe();
    for read in reads {
        processor.process_chunk(read, &silence, sid, &state);
    }
    let mut replay = EgoReplay {
        children: Vec::new(),
        outcomes: Vec::new(),
        questions: 0,
    };
    while let Ok(event) = events.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
            match parsed["type"].as_str() {
                Some("child-activity") => replay
                    .children
                    .push(serde_json::from_value(parsed["children"].clone()).unwrap()),
                Some("turn-outcome") => replay
                    .outcomes
                    .push(parsed["outcome"].as_str().map(str::to_owned)),
                Some("question") => replay.questions += 1,
                _ => {}
            }
        }
    }
    replay
}

fn replay_ego_capture(agent: Option<&str>, scenario: &str) -> EgoReplay {
    replay_ego_reads(agent, &ego_reads(&ego_capture(scenario), None))
}

fn ego_capture(scenario: &str) -> String {
    String::from_utf8_lossy(&agent_prompt_fixture(&format!(
        "ego-native-0.4.3-{scenario}.pty"
    )))
    .into_owned()
}

fn outcomes(settled: &str) -> Vec<Option<String>> {
    vec![Some(settled.to_owned()), None]
}

fn children(rosters: &[&[&str]]) -> Vec<Vec<String>> {
    rosters
        .iter()
        .map(|roster| roster.iter().map(|label| label.to_string()).collect())
        .collect()
}

/// Catches: a recorded turn_end outcome dropped by the PTY consumer, or kept
/// after session_end; a tool call alone raising awaiting or a child.
#[test]
fn ego_native_tool_capture_reports_the_turn_outcome() {
    let replay = replay_ego_capture(Some("ego"), "tool");
    assert_eq!(replay.outcomes, outcomes("completed"));
    assert_eq!(replay.children, [] as [Vec<String>; 0]);
    assert_eq!(
        replay.questions, 0,
        "a tool call alone must not raise awaiting"
    );
}

/// Catches: rich modal frames raising a second wait beside the coarse awaiting frame.
#[test]
fn ego_native_permission_and_question_captures_raise_one_coarse_wait() {
    for scenario in ["permission", "question"] {
        let replay = replay_ego_capture(Some("ego"), scenario);
        assert_eq!(replay.outcomes, outcomes("completed"), "{scenario}");
        assert_eq!(replay.questions, 1, "{scenario}");
    }
}

/// Catches: child start, activity label or end ignored, leaving a finished child on the roster.
#[test]
fn ego_native_child_capture_reports_child_activity_until_end() {
    let replay = replay_ego_capture(Some("ego"), "child");
    assert_eq!(
        replay.children,
        children(&[&[""], &["Working"], &["Responding"], &["Working"], &[]])
    );
    assert_eq!(replay.outcomes, outcomes("completed"));
}

/// Catches: an interrupted or failed turn reported as completed.
#[test]
fn ego_native_cancel_and_error_captures_report_their_outcome() {
    assert_eq!(
        replay_ego_capture(Some("ego"), "cancel").outcomes,
        outcomes("interrupted")
    );
    assert_eq!(
        replay_ego_capture(Some("ego"), "error").outcomes,
        outcomes("failed")
    );
}

/// Catches: an OSC frame cut across reads being dropped or applied twice.
#[test]
fn ego_native_frames_split_across_reads_give_the_same_readout() {
    let whole = replay_ego_capture(Some("ego"), "child");
    for size in [1, 7, 61] {
        let split = replay_ego_reads(Some("ego"), &ego_reads(&ego_capture("child"), Some(size)));
        assert_eq!(split.children, whole.children, "chunk size {size}");
        assert_eq!(split.outcomes, whole.outcomes, "chunk size {size}");
    }
}

/// Catches: the agent-event arm gated on `agent_type`, which a plain shell tab only
/// learns on the 1 s silence tick, so ego's first frames (and its first turn) are lost.
#[test]
fn ego_native_frames_are_accepted_before_the_agent_type_is_known() {
    let known = replay_ego_capture(Some("ego"), "child");
    let early = replay_ego_capture(None, "child");
    assert_eq!(early.children, known.children);
    assert_eq!(early.outcomes, known.outcomes);
}

/// Catches: a second host's frames, a stale turn's late frames or duplicates corrupting the roster.
#[test]
fn ego_native_isolates_sessions_turns_and_duplicates() {
    let (s, t1, t2) = ("sess-a", "turn-1", "turn-2");
    let label = |label: &str| serde_json::json!({ "label": label });
    let reads: Vec<String> = [
        ego_frame(s, None, "session_start", None, serde_json::json!({})),
        ego_frame(s, Some(t1), "turn_start", None, serde_json::json!({})),
        ego_frame(s, Some(t1), "child_start", Some("c"), label("read")),
        // duplicate: no second entry, no new event
        ego_frame(s, Some(t1), "child_start", Some("c"), label("read")),
        // another session: ignored
        ego_frame("sess-b", Some(t1), "child_start", Some("x"), label("exec")),
        // another turn: ignored
        ego_frame(s, Some(t2), "child_activity", Some("c"), label("other")),
        ego_frame(
            s,
            Some(t1),
            "turn_end",
            None,
            serde_json::json!({"outcome": "completed"}),
        ),
        // late frame of the settled turn: must not resurrect work
        ego_frame(s, Some(t1), "child_start", Some("late"), label("late")),
        ego_frame(s, Some(t2), "turn_start", None, serde_json::json!({})),
        ego_frame(s, Some(t2), "child_start", Some("d"), label("exec")),
    ]
    .iter()
    .map(|payload| format!("\x1b]7770;agent-event={payload}\x07"))
    .collect();
    let replay = replay_ego_reads(Some("ego"), &reads);
    assert_eq!(replay.children, children(&[&["read"], &[], &["exec"]]));
    assert_eq!(replay.outcomes, vec![Some("completed".to_owned()), None]);
}

/// Catches: unknown versions, events or agents altering the readout, or a
/// session adopted mid-stream although the contract opens every stream with
/// session_start.
#[test]
fn ego_native_ignores_unknown_frames_and_frames_before_session_start() {
    use base64::Engine;
    let mut events = ego_events::EgoEvents::default();
    let b64 =
        |v: serde_json::Value| base64::engine::general_purpose::STANDARD.encode(v.to_string());
    let frame = |version: u32, agent: &str, event: &str, id: Option<&str>| {
        b64(
            serde_json::json!({"version": version, "agent": agent, "session": "s",
            "turn": "t", "event": event, "id": id, "data": {"label": "child"}}),
        )
    };
    let start = |events: &mut ego_events::EgoEvents| {
        events.receive(&frame(1, "ego", "turn_start", None));
        events.receive(&frame(1, "ego", "child_start", Some("c")));
    };
    start(&mut events);
    assert!(
        events.children().is_empty(),
        "no session adopted mid-stream"
    );
    for (version, agent) in [(2, "ego"), (1, "codex")] {
        events.receive(&frame(version, agent, "session_start", None));
        start(&mut events);
        assert!(events.children().is_empty(), "{agent} v{version}");
    }
    events.receive("not base64 !");
    events.receive(&frame(1, "ego", "session_start", None));
    start(&mut events);
    events.receive(&frame(1, "ego", "future_event", Some("c")));
    assert_eq!(events.children(), ["child"]);
}

/// Catches: a compaction epoch, which ego 0.4.x opens with a fresh `session_start`
/// in the same process (`EpochStart::Compact`), treated as a user stop, so every
/// background completion launched before it is retained and the agent never wakes.
#[test]
fn ego_native_compaction_session_start_keeps_background_wake_origin() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "ego-compaction";
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("ego".to_owned());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 2000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let origin = |state: &AppState| {
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .background_wake_origin
            .clone()
    };
    let mut processor = ChunkProcessor::new(None, None);
    let first = [
        ego_frame("sess-a", None, "session_start", None, serde_json::json!({})),
        ego_frame(
            "sess-a",
            Some("t1"),
            "turn_start",
            None,
            serde_json::json!({}),
        ),
    ];
    for payload in &first {
        processor.process_chunk(
            &format!("\x1b]7770;agent-event={payload}\x07"),
            &silence,
            sid,
            &state,
        );
    }
    // A completion launched here captures this origin.
    let launched = origin(&state);
    let compaction = [
        ego_frame("sess-b", None, "session_start", None, serde_json::json!({})),
        ego_frame(
            "sess-b",
            Some("t2"),
            "turn_start",
            None,
            serde_json::json!({}),
        ),
        ego_frame(
            "sess-b",
            Some("t2"),
            "turn_end",
            None,
            serde_json::json!({"outcome": "completed"}),
        ),
    ];
    for payload in &compaction {
        processor.process_chunk(
            &format!("\x1b]7770;agent-event={payload}\x07"),
            &silence,
            sid,
            &state,
        );
    }
    assert_eq!(
        origin(&state),
        launched,
        "no user stop happened, yet the compaction revoked the launch origin"
    );
    // Non-vacuous: the same pipeline does rotate on a real user stop.
    for payload in [
        ego_frame(
            "sess-b",
            Some("t3"),
            "turn_start",
            None,
            serde_json::json!({}),
        ),
        ego_frame(
            "sess-b",
            Some("t3"),
            "turn_end",
            None,
            serde_json::json!({"outcome": "interrupted", "reason": {"Cancelled": "User"}}),
        ),
    ] {
        processor.process_chunk(
            &format!("\x1b]7770;agent-event={payload}\x07"),
            &silence,
            sid,
            &state,
        );
    }
    assert_ne!(origin(&state), launched, "a user stop must revoke");
}
