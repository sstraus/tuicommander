use super::model::*;
use super::ownership::resolve_owning_project;
use super::store::ProgressStore;

/// Is Progress collecting for this agent?
///
/// One rule, one place: `global AND (per_agent ?? true)`. The same answer gates
/// the MCP tool listing, the `initialize` obligation, the reporting tool and
/// `intent:` capture — a listed tool with no stated obligation, or an
/// obligation naming a tool that is not listed, are the two ways this drifts.
///
/// The global half is read from the live `AppState`, not from disk: a toggle
/// flipped in Settings must take effect on the next report, and `config.json`
/// is only the persistence of that value.
pub fn progress_tracking_enabled(state: &crate::state::AppState, agent_type: Option<&str>) -> bool {
    progress_tracking_enabled_in(
        state.config.read().progress_tracking,
        agent_type,
        &crate::config::load_agents_config(),
    )
}

pub(crate) fn progress_tracking_enabled_in(
    global: bool,
    agent_type: Option<&str>,
    agents: &crate::config::AgentsConfig,
) -> bool {
    global
        && agent_type
            .and_then(|agent| agents.agents.get(agent))
            .and_then(|settings| settings.progress_tracking)
            .unwrap_or(true)
}

/// The error a caller gets when it reports while collection is off. It names
/// the setting, because the agent cannot change it and the user can.
pub(crate) const TRACKING_DISABLED: &str =
    "progress_tracking_disabled: Progress collection is off for this agent (Settings → Agents)";

/// The one write path. Both halves of the journal — what an agent reports and
/// what TUIC observes — pass the same gate, the same validation and the same
/// ownership resolution, in that order. Which kind is allowed in is decided
/// before this, by the caller that built the entry.
fn append(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    entry: NewProgressEntry,
    agent_type: Option<&str>,
    pty_id: Option<&str>,
    target: Option<(&str, Option<&str>)>,
) -> Result<ProgressEntry, String> {
    if !progress_tracking_enabled(state, agent_type) {
        return Err(TRACKING_DISABLED.to_string());
    }
    let project = resolve_owning_project(project_hint)?;
    ProgressStore::open()?.record_hand_off(
        &project.to_string_lossy(),
        &entry,
        pty_id,
        target.map(|(pty, _)| pty),
        target.and_then(|(_, name)| name),
    )
}

/// The registered project a terminal's working directory belongs to.
///
/// A managed worktree outside the repository root answers with its workspace,
/// which the store then files under the parent project.
///
/// `None` when the directory is inside no registered repository or workspace. That is never
/// resolved to the focused UI repository: it would file one terminal's work
/// under whatever the human happened to be looking at.
pub(crate) fn project_for_session(
    state: &crate::state::AppState,
    session_id: &str,
) -> Option<String> {
    let cwd = state
        .session_maps
        .sessions
        .get(session_id)?
        .lock()
        .cwd
        .clone()?;
    let known: Vec<String> = state
        .repo_watchers
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    crate::mcp_http::mcp_transport::registered_repo_for_path(&cwd, &known).or_else(|| {
        super::ownership::registered_workspace_for_path(std::path::Path::new(&cwd))
            .map(|workspace| workspace.to_string_lossy().to_string())
    })
}

/// What an empty hand-off text is journaled as.
pub(crate) const EMPTY_HAND_OFF: &str = "(no text)";

/// What a hand-off entry keeps of a prompt or a message: redacted first, then
/// cut to the journal's cap. The cap is the contract — the full text is not
/// stored anywhere else.
///
/// An empty text becomes `EMPTY_HAND_OFF` rather than a rejected entry: the
/// row is the parent-child edge, and without it the child's outcome is drawn
/// as a note instead of a return to its parent.
pub(crate) fn hand_off_text(text: &str) -> String {
    let redacted = crate::redaction::redact_secrets(text);
    let trimmed = redacted.trim();
    if trimmed.is_empty() {
        return EMPTY_HAND_OFF.to_string();
    }
    if trimmed.chars().count() <= MAX_TEXT_CHARS {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(MAX_TEXT_CHARS - 1)
        .chain(['…'])
        .collect()
}

/// Record that one terminal handed work to another: a `delegated` entry at
/// `agent action=spawn`, a `message` entry at `agent action=send`.
///
/// Filed under the sender's project and gated by the sender's agent, exactly
/// like an `intent:` from that terminal. Lifecycle mail TUIC posts on a
/// child's behalf never comes through here — only an explicit spawn or send.
pub fn record_hand_off(
    state: &crate::state::AppState,
    kind: ProgressKind,
    from_pty: &str,
    to_pty: &str,
    to_name: Option<&str>,
    text: &str,
) -> Result<ProgressEntry, String> {
    if !matches!(kind, ProgressKind::Delegated | ProgressKind::Message) {
        return Err(format!("{} is not a hand-off kind", kind.as_str()));
    }
    let (agent_type, agent_name) = session_identity(state, from_pty);
    let project = project_for_session(state, from_pty);
    append(
        state,
        project.as_deref(),
        NewProgressEntry {
            kind,
            text: hand_off_text(text),
            step: None,
            agent_name,
        },
        agent_type.as_deref(),
        Some(from_pty),
        Some((to_pty, to_name)),
    )
}

/// A terminal's detected agent type and its display name.
pub(crate) fn session_identity(
    state: &crate::state::AppState,
    session_id: &str,
) -> (Option<String>, Option<String>) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let name = state
        .session_maps
        .sessions
        .get(session_id)
        .and_then(|session| session.lock().display_name.clone());
    (agent_type, name)
}

/// Shared reporting core for MCP, HTTP and Tauri IPC. `into_entry` is what
/// refuses `intent` here: an agent may not claim one.
pub fn submit_progress_report(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    input: ProgressReportInput,
    agent_name: Option<String>,
    agent_type: Option<&str>,
    pty_id: Option<&str>,
) -> Result<ProgressEntry, String> {
    append(
        state,
        project_hint,
        input.into_entry(agent_name)?,
        agent_type,
        pty_id,
        None,
    )
}

/// Record an `intent:` marker the agent already emitted.
///
/// This is the host's half of the journal and the reliability floor: the
/// reporting obligation sits hours back in an `initialize` blob, while this
/// trigger fires on every task.
pub fn record_intent(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    text: &str,
    agent_name: Option<String>,
    agent_type: Option<&str>,
    pty_id: Option<&str>,
) -> Result<ProgressEntry, String> {
    let redacted = crate::redaction::redact_secrets(text);
    let journal_text = if redacted.chars().count() > MAX_TEXT_CHARS {
        redacted
            .chars()
            .take(MAX_TEXT_CHARS - 1)
            .chain(['…'])
            .collect()
    } else {
        redacted
    };
    append(
        state,
        project_hint,
        NewProgressEntry {
            kind: ProgressKind::Intent,
            text: journal_text,
            step: None,
            agent_name,
        },
        agent_type,
        pty_id,
        None,
    )
}

pub(crate) fn project_of(project: &str) -> Result<String, String> {
    Ok(resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string())
}

pub fn progress_list(project: &str, input: ProgressListInput) -> Result<ProgressList, String> {
    ProgressStore::open()?.list(&project_of(project)?, &input)
}

pub fn progress_projects() -> Result<Vec<String>, String> {
    ProgressStore::open()?.recent_projects()
}

pub fn progress_delete(
    project: &str,
    input: ProgressDeleteInput,
) -> Result<ProgressDeleteReceipt, String> {
    ProgressStore::open()?.delete(&project_of(project)?, &input.ids)
}

pub fn progress_mark_viewed(
    project: &str,
    pty_id: Option<&str>,
) -> Result<ProgressViewedReceipt, String> {
    ProgressStore::open()?.mark_viewed_for_pty(&project_of(project)?, pty_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentSettings, AgentsConfig};

    fn agents_with(agent: &str, progress_tracking: Option<bool>) -> AgentsConfig {
        let mut agents = AgentsConfig::default();
        agents.agents.insert(
            agent.to_string(),
            AgentSettings {
                progress_tracking,
                ..Default::default()
            },
        );
        agents
    }

    /// `global AND (per_agent ?? true)` — all four corners, plus the two ways
    /// an agent can be silent about it (no entry, or an entry with no opinion).
    #[test]
    fn the_effective_flag_is_global_and_the_agent_override() {
        let opted_out = agents_with("claude", Some(false));
        let opted_in = agents_with("claude", Some(true));
        let no_opinion = agents_with("claude", None);

        assert!(progress_tracking_enabled_in(
            true,
            Some("claude"),
            &opted_in
        ));
        assert!(!progress_tracking_enabled_in(
            true,
            Some("claude"),
            &opted_out
        ));
        assert!(!progress_tracking_enabled_in(
            false,
            Some("claude"),
            &opted_in
        ));
        assert!(!progress_tracking_enabled_in(
            false,
            Some("claude"),
            &opted_out
        ));

        // A missing override means yes; so does an unknown agent and a caller
        // that is not an agent at all (the desktop UI, a local HTTP client).
        assert!(progress_tracking_enabled_in(
            true,
            Some("claude"),
            &no_opinion
        ));
        assert!(progress_tracking_enabled_in(
            true,
            Some("codex"),
            &opted_out
        ));
        assert!(progress_tracking_enabled_in(true, None, &opted_out));
    }

    /// The host writes `intent`, so it is the one kind the journal accepts from
    /// nobody else — and the entry still carries which agent said it.
    #[test]
    fn an_intent_marker_is_recorded_against_the_project_and_the_agent() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();

        let entry = record_intent(
            &state,
            Some(&project.path().to_string_lossy()),
            "Rewriting the Progress store",
            Some("claude".to_string()),
            Some("claude"),
            Some("pty-a"),
        )
        .unwrap();
        assert_eq!(entry.kind, ProgressKind::Intent);
        assert_eq!(entry.text, "Rewriting the Progress store");
        assert_eq!(entry.agent_name.as_deref(), Some("claude"));
        assert_eq!(entry.pty_id.as_deref(), Some("pty-a"));
        assert_eq!(
            entry.project,
            project.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn every_progress_journal_write_redacts_secrets() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        let hint = project.path().to_string_lossy();
        let secret = format!("ghp_{}", "A".repeat(40));
        let intent = record_intent(
            &state,
            Some(&hint),
            &format!("Inspect {secret}"),
            None,
            Some("codex"),
            None,
        )
        .unwrap();
        assert_eq!(intent.text, "Inspect [REDACTED]");
        for kind in [ProgressKind::Done, ProgressKind::Blocked] {
            let report = submit_progress_report(
                &state,
                Some(&hint),
                ProgressReportInput {
                    kind,
                    text: format!("Handled {secret}"),
                    step: Some(format!("step {secret}")),
                },
                None,
                Some("codex"),
                None,
            )
            .unwrap();
            assert_eq!(report.text, "Handled [REDACTED]");
            assert_eq!(report.step.as_deref(), Some("step [REDACTED]"));
        }
        for kind in [ProgressKind::Delegated, ProgressKind::Message] {
            let hand_off = ProgressStore::open()
                .unwrap()
                .record_hand_off(
                    &project.path().canonicalize().unwrap().to_string_lossy(),
                    &NewProgressEntry {
                        kind,
                        text: format!("Review {secret}"),
                        step: None,
                        agent_name: None,
                    },
                    Some("pty-a"),
                    Some("pty-b"),
                    None,
                )
                .unwrap();
            assert_eq!(hand_off.text, "Review [REDACTED]");
        }
        let stored = progress_list(&hint, ProgressListInput::default()).unwrap();
        assert!(
            stored
                .entries
                .iter()
                .all(|entry| !entry.text.contains(&secret))
        );
    }

    #[test]
    fn redaction_precedes_progress_length_validation() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        let hint = project.path().to_string_lossy();
        let secret = format!("ghp_{}", "A".repeat(40));
        let raw = format!("{} {secret}", "a".repeat(470));
        assert!(raw.chars().count() > MAX_TEXT_CHARS);
        let entry = record_intent(&state, Some(&hint), &raw, None, Some("codex"), None)
            .expect("redacted intent fits the journal cap");
        assert!(!entry.text.contains(&secret));
        assert!(entry.text.chars().count() <= MAX_TEXT_CHARS);
    }

    #[test]
    fn progress_names_are_bounded_before_storage() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        let hint = project.path().to_string_lossy();
        let long_name = "n".repeat(1000);
        let secret = format!("ghp_{}", "A".repeat(40));
        let intent = record_intent(
            &state,
            Some(&hint),
            "Inspect the journal",
            Some(long_name.clone()),
            Some("codex"),
            None,
        )
        .unwrap();
        assert_eq!(intent.agent_name.as_deref().unwrap().chars().count(), 80);
        let redacted_name = record_intent(
            &state,
            Some(&hint),
            "Inspect the credentials",
            Some(format!("Agent {secret}")),
            Some("codex"),
            None,
        )
        .unwrap();
        assert_eq!(
            redacted_name.agent_name.as_deref(),
            Some("Agent [REDACTED]")
        );
        let hand_off = ProgressStore::open()
            .unwrap()
            .record_hand_off(
                &project.path().canonicalize().unwrap().to_string_lossy(),
                &NewProgressEntry {
                    kind: ProgressKind::Delegated,
                    text: "Review this".into(),
                    step: None,
                    agent_name: None,
                },
                Some("pty-a"),
                Some("pty-b"),
                Some(&long_name),
            )
            .unwrap();
        assert_eq!(hand_off.target_name.as_deref().unwrap().chars().count(), 80);
        let unicode_name = "é".repeat(81);
        let unicode_hand_off = ProgressStore::open()
            .unwrap()
            .record_hand_off(
                &project.path().canonicalize().unwrap().to_string_lossy(),
                &NewProgressEntry {
                    kind: ProgressKind::Message,
                    text: "Review this".into(),
                    step: None,
                    agent_name: None,
                },
                Some("pty-a"),
                Some("pty-b"),
                Some(&unicode_name),
            )
            .unwrap();
        let expected_name = "é".repeat(80);
        assert_eq!(
            unicode_hand_off.target_name.as_deref(),
            Some(expected_name.as_str())
        );
        let space_at_limit = format!("{} suffix", "n".repeat(MAX_NAME_CHARS - 1));
        let bounded = record_intent(
            &state,
            Some(&hint),
            "Inspect the name limit",
            Some(space_at_limit),
            Some("codex"),
            None,
        )
        .unwrap();
        assert_eq!(bounded.agent_name.as_deref(), Some("n".repeat(79).as_str()));
    }

    /// Collection off means the journal does not grow — not that it grows more
    /// quietly. Both the reporting path and the host's own capture stop here.
    #[test]
    fn nothing_is_written_while_collection_is_off() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().progress_tracking = false;

        let hint = project.path().to_string_lossy().to_string();
        assert_eq!(
            record_intent(&state, Some(&hint), "Should not land", None, None, None).unwrap_err(),
            TRACKING_DISABLED
        );
        assert_eq!(
            submit_progress_report(
                &state,
                Some(&hint),
                ProgressReportInput {
                    kind: ProgressKind::Done,
                    text: "Should not land either".to_string(),
                    step: None,
                },
                None,
                None,
                None,
            )
            .unwrap_err(),
            TRACKING_DISABLED
        );
        assert_eq!(
            progress_list(&hint, ProgressListInput::default())
                .unwrap()
                .entries,
            [] as [crate::progress::model::ProgressEntry; 0]
        );
    }

    /// A prompt handed to a child can carry a secret the parent was given. The
    /// journal is a file read by a dialog, so the secret is redacted before the
    /// 500-character cut — cutting first could split it past its pattern.
    #[test]
    fn a_hand_off_text_is_redacted_before_it_is_capped() {
        let pad = "x".repeat(MAX_TEXT_CHARS - 10);
        let text = format!("{pad} ghp_{}", "A".repeat(40));
        let kept = hand_off_text(&text);
        assert!(!kept.contains("ghp_"), "{kept}");
        assert!(kept.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(hand_off_text("  short  "), "short");
    }

    /// `agent action=spawn` accepts an empty prompt. The hand-off row is the
    /// only record of who spawned whom, so it must still pass validation.
    #[test]
    fn an_empty_hand_off_text_still_journals_the_edge() {
        for text in ["", "  \n\t "] {
            let kept = hand_off_text(text);
            assert_eq!(kept, EMPTY_HAND_OFF);
            NewProgressEntry {
                kind: ProgressKind::Delegated,
                text: kept,
                step: None,
                agent_name: None,
            }
            .validate()
            .expect("an empty prompt still records the delegation");
        }
    }

    /// Only a spawn and a send are hand-offs; the other kinds have their own
    /// writers and must not be forged through this one.
    #[test]
    fn record_hand_off_refuses_a_kind_that_is_not_a_hand_off() {
        let state = crate::state::tests_support::make_test_app_state();
        for kind in [
            ProgressKind::Done,
            ProgressKind::Blocked,
            ProgressKind::Intent,
        ] {
            assert!(record_hand_off(&state, kind, "a", "b", None, "x").is_err());
        }
    }

    /// A PTY runs wherever the user started it. When that directory belongs to
    /// no registered repository there is no project to attribute an `intent:`
    /// to, and the focused UI repository is emphatically not the answer — it
    /// would file one tab's work under whatever the user happened to be looking
    /// at. The caller resolves the project through `registered_repo_for_path`
    /// and drops the marker when it answers `None`.
    #[test]
    fn an_unregistered_working_directory_resolves_to_no_project() {
        use crate::mcp_http::mcp_transport::registered_repo_for_path;

        let known = vec![
            "/Users/dev/one".to_string(),
            "/Users/dev/one/two".to_string(),
        ];
        assert_eq!(registered_repo_for_path("/tmp/scratch", &known), None);
        assert_eq!(
            registered_repo_for_path("/Users/dev/one/src/lib.rs", &known).as_deref(),
            Some("/Users/dev/one")
        );
        // The deepest registered repository wins, so a nested registration is
        // never filed under its parent.
        assert_eq!(
            registered_repo_for_path("/Users/dev/one/two/src", &known).as_deref(),
            Some("/Users/dev/one/two")
        );
        // A sibling that merely shares a prefix is not inside anything.
        assert_eq!(registered_repo_for_path("/Users/dev/oneiric", &known), None);
    }
}
