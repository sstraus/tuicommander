use crate::state::AppState;
use std::sync::Arc;
#[cfg(all(test, unix))]
use tuic_dictation::continuous::VoiceHold;
use tuic_dictation::continuous::{TargetProbe, VoiceSink, VoiceWrite};

/// Production adapter: `pty::write_voice_turn`, nothing else.
pub struct PtyVoiceSink<'a>(pub &'a AppState);

impl VoiceSink for PtyVoiceSink<'_> {
    fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String> {
        crate::pty::write_voice_turn(self.0, session_id, text)
    }
}

/// Production probe: the same predicate `arm` checked, re-asked every tick.
#[cfg(all(test, unix))]
pub struct PtyTargetProbe<'a>(pub &'a AppState);

#[cfg(all(test, unix))]
impl TargetProbe for PtyTargetProbe<'_> {
    fn accepts(&self, session_id: &str) -> bool {
        crate::pty::session_accepts_voice(self.0, session_id)
    }
}

/// Owned form of the PTY ports used by the hands-free worker thread.
pub struct PtyVoicePort(pub Arc<AppState>);

impl VoiceSink for PtyVoicePort {
    fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String> {
        crate::pty::write_voice_turn(&self.0, session_id, text)
    }
}

impl TargetProbe for PtyVoicePort {
    fn accepts(&self, session_id: &str) -> bool {
        crate::pty::session_accepts_voice(&self.0, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Catches: held phrases losing their line boundary or submitting separately.
    #[cfg(unix)]
    #[test]
    fn queued_voice_phrases_keep_soft_newlines_and_submit_once() {
        use tuic_dictation::continuous::{HandsFree, deliver_due};

        let cases: &[(&[&str], &[u8])] = &[
            (
                &["first phrase", "second phrase"],
                b"\x15\x1b[200~first phrase\nsecond phrase\x1b[201~\r",
            ),
            (&["first phrase"], b"\x15first phrase\r"),
            (
                &["first phrase", "second phrase", "third phrase"],
                b"\x15\x1b[200~first phrase\nsecond phrase\nthird phrase\x1b[201~\r",
            ),
            (
                &["first phrase\n", "second phrase"],
                b"\x15\x1b[200~first phrase\nsecond phrase\x1b[201~\r",
            ),
        ];
        for (agent, (phrases, expected)) in ["claude", "codex"]
            .into_iter()
            .flat_map(|agent| cases.iter().map(move |case| (agent, case)))
        {
            let state = crate::state::tests_support::make_test_app_state();
            let session = "voice-lines";
            crate::test_support::agent_session(&state, session, crate::pty::SHELL_BUSY);
            let bytes = crate::test_support::insert_recording_session(&state, session);
            {
                let mut target = state.session_maps.session_states.get_mut(session).unwrap();
                target.agent_type = Some(agent.into());
                target.question_confident = true;
            }
            let mut mode = HandsFree::new(1_500);
            let generation = mode.arm(session, "test", true).unwrap();
            mode.accept_transcript(generation, phrases[0], None, 0);
            let sink = PtyVoiceSink(&state);
            deliver_due(&mut mode, &sink, 1_500);
            assert!(bytes.lock().unwrap().is_empty());
            for phrase in &phrases[1..] {
                mode.accept_transcript(generation, phrase, None, 2_000);
            }
            state
                .session_maps
                .session_states
                .get_mut(session)
                .unwrap()
                .question_confident = false;
            deliver_due(&mut mode, &sink, 3_500);
            assert_eq!(
                bytes.lock().unwrap().as_slice(),
                *expected,
                "{agent}: phrase boundaries are paste content, only the final CR submits"
            );
            deliver_due(&mut mode, &sink, u64::MAX);
            assert_eq!(
                bytes.lock().unwrap().as_slice(),
                *expected,
                "a later tick must not submit the same turn twice"
            );
        }
    }

    /// "Unsupported targets stay unavailable" is a refusal at the sink, not a
    /// fallback somewhere else: there is no other exit from this module.
    #[test]
    fn the_real_sink_refuses_a_target_that_cannot_take_hands_free_input() {
        let state = crate::state::tests_support::make_test_app_state();

        assert_eq!(
            PtyVoiceSink(&state).write("no-such-session", "hello"),
            Err("Session not found".to_string())
        );
        assert_eq!(
            PtyVoiceSink(&state).write("no-such-session", "   "),
            Err("Command text is empty".to_string())
        );
    }

    /// Boss's rule against the real sink: a working agent takes the turn at
    /// once, as it takes a line typed by hand; a dialog holds it. The idle case
    /// is the control — without it "typed" would be equally true of a harness
    /// that types into anything.
    ///
    /// The dialog row is the one with a user-visible failure behind it: a raw
    /// write would answer an open permission prompt with whatever the user
    /// happened to say in the room. None of the three ends the conversation.
    #[cfg(unix)]
    #[test]
    fn a_busy_target_takes_the_turn_a_dialog_holds_it_and_all_stay_targets() {
        let state = crate::state::tests_support::make_test_app_state();
        for (session, shell) in [
            ("voice-idle", crate::pty::SHELL_IDLE),
            ("voice-busy", crate::pty::SHELL_BUSY),
            ("voice-dialog", crate::pty::SHELL_IDLE),
        ] {
            crate::test_support::agent_session(&state, session, shell);
            crate::test_support::insert_recording_session(&state, session);
        }
        // Idle, but a confident question owns the composer.
        state
            .session_maps
            .session_states
            .get_mut("voice-dialog")
            .expect("the session was just inserted")
            .question_confident = true;

        let spoken = |session: &str| {
            PtyVoiceSink(&state)
                .write(session, "esegui i test")
                .expect("a live agent session takes the entry")
        };

        assert_eq!(spoken("voice-idle"), VoiceWrite::Written);
        assert_eq!(
            spoken("voice-busy"),
            VoiceWrite::Written,
            "a working agent takes a spoken turn mid-turn, as it takes a typed line"
        );
        assert_eq!(
            spoken("voice-dialog"),
            VoiceWrite::Held(VoiceHold::Question),
            "an open prompt must not be answered with speech the user aimed at the agent"
        );
        for session in ["voice-idle", "voice-busy", "voice-dialog"] {
            assert_eq!(
                crate::pty::queued_command_count(&state, session),
                0,
                "{session}: speech never enters the Compose queue"
            );
            assert!(
                PtyTargetProbe(&state).accepts(session),
                "{session} is still a target"
            );
        }
    }
}
