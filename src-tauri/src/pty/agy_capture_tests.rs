// agy 1.5.10, recorded 2026-11-22 through `scripts/agent_capture` (isolated
// debug daemon, throwaway workspace). See `fixtures/agent_prompts/agy-1.5.10.md`.

/// Screen verdicts of a capture, `Unknown` dropped and repeats folded, split at
/// each recorded user input.
fn agy_activity_between_inputs(fixture: &str) -> Vec<Vec<AgentScreenActivity>> {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture)).unwrap();
    let (rows, cols) = capture.geometry.expect("real recorded geometry");
    let mut vt_log = crate::state::VtLogBuffer::new(rows, cols, 2000);
    let mut phases = vec![Vec::new()];
    for record in &capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            phases.push(Vec::new());
            continue;
        }
        vt_log.process(&record.data);
        let activity = detect_agent_screen_activity(Some("agy"), &vt_log.screen_rows());
        let phase = phases.last_mut().unwrap();
        if activity != AgentScreenActivity::Unknown && phase.last() != Some(&activity) {
            phase.push(activity);
        }
    }
    phases
}

/// Catches: the trust picker's `> Yes, I trust this folder` row read as a ready
/// composer, and the composer that stays visible during a turn read as Ready.
#[test]
fn agy_trust_turn_capture_is_ready_only_at_the_composer() {
    use AgentScreenActivity::{Ready, Working};
    let phases = agy_activity_between_inputs("agy-1.5.10-trust-turn.tcap");
    // Inputs: Enter on the trust picker, the prompt text, Enter.
    assert_eq!(phases.len(), 4);
    assert_eq!(phases[0], [], "the trust picker is not a composer");
    assert_eq!(phases[1], [Ready], "trusted workspace, empty composer");
    assert_eq!(
        phases[2],
        [],
        "the text and its Enter were written together"
    );
    assert_eq!(
        phases[3],
        [Ready, Working, Ready],
        "the draft echo, one turn, then the empty composer"
    );
}

/// Catches: `Model unavailable … retrying` rows, drawn above a visible empty
/// composer for minutes, read as a finished turn.
#[test]
fn agy_model_retry_capture_stays_working_after_submit() {
    let phases = agy_activity_between_inputs("agy-1.5.10-model-unavailable.tcap");
    let after_submit = phases.last().unwrap();
    assert_eq!(
        after_submit[..2],
        [AgentScreenActivity::Ready, AgentScreenActivity::Working]
    );
    assert!(
        !after_submit[2..].contains(&AgentScreenActivity::Ready),
        "the retry loop never returned to the composer: {after_submit:?}"
    );
}

/// What a client sees at each recorded user input: the open picker, if any.
fn agy_prompts_at_inputs(
    fixture: &str,
) -> (Vec<Option<crate::output_parser::ChoicePromptPayload>>, bool) {
    let mut prompts = Vec::new();
    let mut final_awaiting = false;
    replay_native_question(fixture, "agy", |record, session| {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            assert_eq!(session.awaiting_input, session.choice_prompt.is_some());
            prompts.push(session.choice_prompt.clone());
        }
        final_awaiting = session.awaiting_input;
    });
    (prompts, final_awaiting)
}

fn agy_labels(prompt: &crate::output_parser::ChoicePromptPayload) -> Vec<&str> {
    prompt
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect()
}

/// Catches: an agy session that waits on its workspace-trust picker while the
/// tab shows idle.
#[tokio::test]
async fn agy_trust_picker_awaits_until_enter() {
    let (prompts, final_awaiting) = agy_prompts_at_inputs("agy-1.5.10-trust-turn.tcap");
    let trust = prompts[0]
        .as_ref()
        .expect("trust picker at the first Enter");
    assert_eq!(trust.title, "Do you trust the contents of this project?");
    assert_eq!(agy_labels(trust), ["Yes, I trust this folder", "No, exit"]);
    assert!(trust.options[0].highlighted && trust.options[1].destructive);
    assert_eq!(
        trust.selection_mode,
        Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
    );
    assert_eq!(prompts[1..], [None, None], "Enter settled the picker");
    assert!(!final_awaiting);
}

/// Catches: a command approval that leaves the session working with no wait,
/// or a wait that survives the approving Enter.
#[tokio::test]
async fn agy_command_approval_awaits_with_the_command_and_retracts() {
    let (prompts, final_awaiting) = agy_prompts_at_inputs("agy-1.5.10-command-approval.tcap");
    // Inputs: trust Enter, prompt text, Enter, approval Enter.
    assert_eq!(prompts.len(), 4);
    let approval = prompts[3]
        .as_ref()
        .expect("approval picker at the last Enter");
    assert_eq!(approval.title, "Run this command? date -u");
    assert_eq!(
        agy_labels(approval),
        [
            "Yes, run command",
            "Yes, and always allow in this conversation for commands that start with 'date'",
            "Yes, and always allow for commands that start with 'date' (Persist to settings.json)",
            "No, cancel",
        ]
    );
    assert!(approval.options[3].destructive);
    assert_eq!(approval.amend_key.as_deref(), Some("amend"));
    assert!(!final_awaiting, "the approving Enter retracts the wait");
}

/// Catches: the native question form missing from clients, and a wait that
/// clears on the arrow key instead of the selecting Enter.
#[tokio::test]
async fn agy_native_question_awaits_through_navigation_until_enter() {
    let (prompts, final_awaiting) = agy_prompts_at_inputs("agy-1.5.10-question.tcap");
    // Inputs: trust Enter, prompt text, Enter, Down, Enter.
    assert_eq!(prompts.len(), 5);
    let question = prompts[3].as_ref().expect("question form at the arrow key");
    assert_eq!(question.title, "Select a colour:");
    assert_eq!(
        agy_labels(question),
        ["red", "green", "blue", "Write-in..."]
    );
    assert!(question.options[0].highlighted);
    // Cursor movement does not publish the prompt again; the wait must hold.
    assert_eq!(
        prompts[4].as_ref(),
        Some(question),
        "still open at the selecting Enter"
    );
    assert!(!final_awaiting);
}
