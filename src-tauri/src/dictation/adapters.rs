use crate::state::AppState;
use std::sync::Arc;
use tuic_dictation::continuous::VoiceHold;
use tuic_dictation::continuous::{TargetProbe, VoiceSink, VoiceWrite};

/// Opaque binding ids keep ACP conversations out of PTY identity and routing.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatVoiceTarget {
    pub id: String,
    pub name: String,
    pub chat_session_id: String,
}

fn chat_id(connection: crate::acp::AcpConnectionId, session: &str) -> String {
    format!("acp:{connection}:{session}")
}

pub fn chat_targets(state: &AppState) -> Vec<ChatVoiceTarget> {
    state
        .acp
        .chat_snapshots()
        .into_iter()
        .flat_map(|connection| {
            connection
                .attachments
                .into_iter()
                .filter(|attachment| live_attachment(attachment.state))
                .map(move |attachment| {
                    let session = attachment.session_id.to_string();
                    ChatVoiceTarget {
                        id: chat_id(connection.connection_id, &session),
                        name: format!("AI Chat · {session}"),
                        chat_session_id: session,
                    }
                })
        })
        .collect()
}

fn live_attachment(state: crate::acp::AcpAttachmentState) -> bool {
    use crate::acp::AcpAttachmentState::{Cancelling, Idle, PausePending, Paused, Prompting};
    matches!(state, Idle | Prompting | Cancelling | PausePending | Paused)
}

fn chat_target(
    state: &AppState,
    target: &str,
) -> Option<(
    crate::acp::AcpConnectionId,
    crate::acp::AcpAttachmentSnapshot,
)> {
    state
        .acp
        .chat_snapshots()
        .into_iter()
        .find_map(|connection| {
            connection
                .attachments
                .into_iter()
                .find(|attachment| {
                    live_attachment(attachment.state)
                        && chat_id(connection.connection_id, &attachment.session_id.to_string())
                            == target
                })
                .map(|attachment| (connection.connection_id, attachment))
        })
}

/// The native MCP channel must own the exact live connection generation.
/// Never resolve ACP through a peer's latest conversation or a terminal id.
pub fn chat_caller(state: &AppState, origin: &crate::acp::AcpMcpOrigin, target: &str) -> bool {
    state.acp.chat_snapshots().into_iter().any(|connection| {
        connection.connection_id == origin.connection_id
            && connection.generation == origin.generation
            // DEFERRED (2026-10-31) — MCP-over-ACP origins name a connection,
            // not a session. While another conversation has a running turn,
            // its tool call cannot be attributed to this voice target.
            && !connection.attachments.iter().any(|attachment| {
                chat_id(connection.connection_id, &attachment.session_id.to_string()) != target
                    && attachment.active_turn.as_ref().is_some_and(|turn| turn.state != crate::acp::AcpTurnState::Settled)
            })
            && connection.attachments.iter().any(|attachment| {
                live_attachment(attachment.state)
                    && chat_id(connection.connection_id, &attachment.session_id.to_string())
                        == target
            })
    })
}

pub fn accepts_voice(state: &AppState, target: &str) -> bool {
    if target.starts_with("acp:") {
        chat_target(state, target).is_some()
    } else {
        crate::pty::session_accepts_voice(state, target)
    }
}

fn write_voice(state: &AppState, target: &str, text: &str) -> Result<VoiceWrite, String> {
    if !target.starts_with("acp:") {
        return crate::pty::write_voice_turn(state, target, text);
    }
    if text.trim().is_empty() {
        return Err("Command text is empty".into());
    }
    let (connection, attachment) =
        chat_target(state, target).ok_or("AI Chat target is no longer attached")?;
    if !attachment.pending_permission_ids.is_empty()
        || !attachment.pending_elicitation_ids.is_empty()
    {
        return Ok(VoiceWrite::Held(VoiceHold::Question));
    }
    if matches!(
        attachment.state,
        crate::acp::AcpAttachmentState::Paused | crate::acp::AcpAttachmentState::PausePending
    ) {
        return Ok(VoiceWrite::Held(VoiceHold::InFlight));
    }
    // Identical to acp_session_prompt (typed input): the actor owns queuing,
    // transcript publication, cancellation and the actual session/prompt call.
    tauri::async_runtime::block_on(state.acp.prompt_with_context(
        connection,
        attachment.session_id,
        vec![agent_client_protocol::schema::v1::ContentBlock::Text(
            agent_client_protocol::schema::v1::TextContent::new(text),
        )],
        None,
    ))
    .map_err(|error| error.message)?;
    Ok(VoiceWrite::Written)
}

/// Production adapter: `pty::write_voice_turn`, nothing else.
pub struct PtyVoiceSink<'a>(pub &'a AppState);

impl VoiceSink for PtyVoiceSink<'_> {
    fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String> {
        write_voice(self.0, session_id, text)
    }
}

/// Production probe: the same predicate `arm` checked, re-asked every tick.
#[cfg(all(test, unix))]
pub struct PtyTargetProbe<'a>(pub &'a AppState);

#[cfg(all(test, unix))]
impl TargetProbe for PtyTargetProbe<'_> {
    fn accepts(&self, session_id: &str) -> bool {
        accepts_voice(self.0, session_id)
    }
}

/// Owned form of the PTY ports used by the hands-free worker thread.
pub struct PtyVoicePort(pub Arc<AppState>);

impl VoiceSink for PtyVoicePort {
    fn write(&self, session_id: &str, text: &str) -> Result<VoiceWrite, String> {
        write_voice(&self.0, session_id, text)
    }
}

impl TargetProbe for PtyVoicePort {
    fn accepts(&self, session_id: &str) -> bool {
        accepts_voice(&self.0, session_id)
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
