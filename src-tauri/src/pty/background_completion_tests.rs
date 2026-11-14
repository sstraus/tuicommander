//! Recorded native stop frames, the real queue gate and observable PTY bytes.
//! Frames were extracted without modification from terminal ecffe3ff on
//! 2026-11-12 (native session 01a1cdaf-5e76-76cf-8758-e0ce876f27e4).

use super::*;

#[cfg(unix)]
#[test]
fn native_stop_revokes_prequeued_and_late_completions_but_preserves_explicit_input() {
    // Catches: arrival-only suppression, a later turn rearming old work,
    // text-prefix suppression of user input, and retry duplication.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "native-background-stop";
    crate::test_support::agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("ego".into());
    let bytes = crate::test_support::insert_recording_session(&state, sid);
    let mut processor = ChunkProcessor::new(None, None);
    let mut frames = include_str!("../fixtures/agent_prompts/ego-user-stop.frames").lines();
    processor.receive_ego_background_event(&state, sid, frames.next().unwrap());
    let old_origin = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .background_wake_origin
        .0
        .clone();
    enqueue_automatic_completion(
        &state,
        sid,
        "OLD-AUTOMATIC-RESULT",
        Some("old-job"),
        &old_origin,
    )
    .unwrap();
    enqueue_user_command(&state, sid, "BG DONE is my explicit request", None).unwrap();
    assert_eq!(list_queued_commands(&state, sid).len(), 2);
    assert!(bytes.lock().unwrap().is_empty());
    for frame in frames {
        processor.receive_ego_background_event(&state, sid, frame);
    }
    assert_eq!(processor.ego_events.outcome(), Some("interrupted"));
    let new_origin = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .background_wake_origin
        .0
        .clone();
    assert_ne!(new_origin, old_origin);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .confirm_idle();
    flush_pending_injections_blocking(&state, sid);
    let delivered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(
        delivered.contains("BG DONE is my explicit request"),
        "{delivered:?}"
    );
    assert!(!delivered.contains("OLD-AUTOMATIC-RESULT"), "{delivered:?}");
    assert!(list_queued_commands(&state, sid).is_empty());

    // The explicit request has now completed; old work still has no authority.
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .confirm_idle();
    let before = bytes.lock().unwrap().clone();
    for _ in 0..2 {
        let receipt = enqueue_automatic_completion(
            &state,
            sid,
            "LATE-OLD-RESULT",
            Some("late-job"),
            &old_origin,
        )
        .unwrap();
        assert!(receipt.accepted && receipt.retained && !receipt.typed);
    }
    assert_eq!(*bytes.lock().unwrap(), before);
    assert!(list_queued_commands(&state, sid).is_empty());

    // Freshly admitted work remains eligible and retains keyed retry identity.
    transition_explicit_shell_state(&state, sid, SHELL_BUSY, "busy", true);
    for _ in 0..2 {
        let receipt = enqueue_automatic_completion(
            &state,
            sid,
            "NEW-AUTOMATIC-RESULT",
            Some("new-job"),
            &new_origin,
        )
        .unwrap();
        assert!(!receipt.retained);
    }
    assert_eq!(list_queued_commands(&state, sid).len(), 1);
}
