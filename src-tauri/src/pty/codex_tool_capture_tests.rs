// Catches: real native producer frames failing to enter/settle the structured question path.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_recorded_transport_reaches_question_and_settlement() {
    let sid = "codex-recorded-tool-stream";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.tuic = "codex-recorded-owner".into();
    tab.processor.tuic_session = Some(tab.tuic.clone());
    let capture = crate::pty_capture::decode_capture(include_bytes!(
        "../fixtures/agent_prompts/codex-native-tools-2626.tcap"
    ))
    .unwrap();
    assert_eq!(capture.geometry, Some((24, 100)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut events = state.event_bus.subscribe();
    let mut questions = 0;
    let mut cleared = false;
    for record in capture.records {
        assert_eq!(
            record.direction,
            crate::pty_capture::CaptureDirection::Output
        );
        tab.processor.process_chunk(
            std::str::from_utf8(&record.data).unwrap(),
            &silence,
            sid,
            &state,
        );
        while let Ok(event) = events.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
                if parsed["type"] == "question" {
                    questions += 1;
                    assert_eq!(parsed["prompt_text"], "Pick a color");
                    assert_eq!(parsed["confident"], true);
                }
                if questions > 0 && parsed["type"] == "user-input" {
                    cleared = true;
                }
            }
        }
    }
    assert_eq!(questions, 1);
    assert!(cleared, "matching native answer must retract the question");
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
}
