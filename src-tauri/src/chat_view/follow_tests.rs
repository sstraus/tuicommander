//! Real-file follow regressions. Schema comes from the recorded Claude corpus.

use super::*;
use crate::state::AppEvent;

const ROW: &str = include_str!("../fixtures/chat_view/recorded/shape-012.jsonl");

/// Real foreground PID/env and disk discovery, isolated from the user's Claude store.
#[cfg(unix)]
struct DiscoveredTerminal {
    state: Arc<crate::state::AppState>,
    _scratch: tempfile::TempDir,
    project: PathBuf,
}

#[cfg(unix)]
impl DiscoveredTerminal {
    fn new() -> Self {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let cwd = scratch.path().to_str().unwrap();
        let config = scratch.path().join("claude");
        let project = crate::agent_session::claude_project_dir_path(cwd, config.to_str()).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        let executable = scratch.path().join("cat-probe");
        std::fs::copy("/bin/cat", &executable).unwrap();
        // Apple's protected binaries do not expose their environment to sysctl.
        #[cfg(target_os = "macos")]
        assert!(
            std::process::Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        let mut command = CommandBuilder::new(&executable);
        command.env("CLAUDE_CONFIG_DIR", &config);
        let mut child = pair.slave.spawn_command(command).unwrap();
        let pid = child.process_id().unwrap();
        // Setup has no behavioral deadline; nextest bounds a genuine hang.
        while pair.master.process_group_leader() != Some(pid as libc::pid_t)
            || crate::agent_session::read_agent_env_overrides("claude", pid)
                .get("CLAUDE_CONFIG_DIR")
                != config.to_str().map(str::to_owned).as_ref()
        {
            assert!(
                child.try_wait().unwrap().is_none(),
                "probe exited during setup"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.session_maps.sessions.insert(
            "discovered".into(),
            Mutex::new(crate::state::PtySession {
                launch_receipt: None,
                writer: Arc::new(Mutex::new(pair.master.take_writer().unwrap())),
                master: pair.master,
                _child: child,
                paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                worktree: None,
                initial_cwd: Some(cwd.into()),
                cwd: Some(cwd.into()),
                display_name: None,
                display_name_is_custom: false,
                display_name_from_spawn: false,
                is_remote: false,
                shell: "/bin/cat".into(),
            }),
        );
        crate::test_support::agent_session(&state, "discovered", crate::pty::SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut("discovered")
            .unwrap()
            .agent_type = Some("claude".into());
        Self {
            state,
            _scratch: scratch,
            project,
        }
    }

    fn transcript(&self, uuid: &str, text: &str) -> PathBuf {
        let path = self.project.join(format!("{uuid}.jsonl"));
        std::fs::write(&path, fixture_row(uuid, text)).unwrap();
        path
    }

    fn snapshot(&self, cursor: Option<&ChatViewSnapshot>) -> Result<ChatViewSnapshot, String> {
        chat_view_snapshot(
            &self.state,
            "discovered",
            cursor.map(|c| c.epoch),
            cursor.map_or(0, |c| c.next_seq),
        )
    }
}

#[cfg(unix)]
impl Drop for DiscoveredTerminal {
    fn drop(&mut self) {
        if let Some((_, session)) = self.state.session_maps.sessions.remove("discovered") {
            let mut session = session.lock();
            session._child.kill().unwrap();
            session._child.wait().unwrap();
        }
    }
}

// Catches: a live parent JSONL losing its binding at recheck solely because it has no subagents.
#[cfg(unix)]
#[test]
fn discovered_transcript_keeps_following_without_subagents_at_recheck() {
    let terminal = DiscoveredTerminal::new();
    let uuid = "11111111-1111-4111-8111-111111111111";
    let path = terminal.transcript(uuid, "before");
    let subagents = terminal.project.join(uuid).join("subagents");
    std::fs::create_dir_all(&subagents).unwrap();
    let first = terminal.snapshot(None).expect("initial discovery");
    assert_eq!(first.updates[0]["content"]["text"], "before");
    std::fs::remove_dir(&subagents).unwrap();
    terminal
        .state
        .chat_views
        .views
        .lock()
        .get("discovered")
        .unwrap()
        .lock()
        .bound_at = Instant::now() - REBIND_EVERY;
    append(&path, &fixture_row("append", "after recheck"));
    let next = terminal
        .snapshot(Some(&first))
        .expect("parent transcript must survive recheck without subagents");
    assert!(!next.reset);
    assert_eq!(next.updates[0]["content"]["text"], "after recheck");
}

fn fixture_row(id: &str, text: &str) -> String {
    let mut row: Value = serde_json::from_str(ROW).expect("recorded row");
    row["uuid"] = Value::String(id.into());
    row["message"]["id"] = Value::String(id.into());
    row["message"]["content"][0]["text"] = Value::String(text.into());
    format!("{row}\n")
}

fn append(path: &Path, row: &str) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open")
        .write_all(row.as_bytes())
        .expect("append");
}

async fn changed(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>, sid: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::ChatViewChanged { session_id, .. } =
                rx.recv().await.expect("event bus")
            {
                assert_eq!(session_id, sid, "wake belongs to the watched terminal");
                return;
            }
        }
    })
    .await
    .expect("ticker did not wake the watched chat after the file changed");
}

// Catches: a same-path replacement of equal size stays at EOF, freezing the chat
// at the old conversation even though the file and later rows belong to a new one.
#[tokio::test]
async fn ticker_follows_appends_and_equal_size_transcript_replacement() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
    let path = tmp.path().join("transcript.jsonl");
    std::fs::write(&path, fixture_row("a", "before")).expect("write");
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "follow-test";
    state.chat_views.views.lock().insert(
        sid.into(),
        Arc::new(Mutex::new(View::new(
            path.clone(),
            MAX_LOG_ENTRIES,
            MAX_LOG_BYTES,
        ))),
    );
    let mut rx = state.event_bus.subscribe();
    let initial = chat_view_snapshot_blocking(state.clone(), sid.into(), None, 0)
        .await
        .expect("initial");
    assert_eq!(initial.updates[0]["content"]["text"], "before");
    append(&path, &fixture_row("b", "append"));
    changed(&mut rx, sid).await;
    let appended = chat_view_snapshot_blocking(
        state.clone(),
        sid.into(),
        Some(initial.epoch),
        initial.next_seq,
    )
    .await
    .expect("appended");
    assert!(!appended.reset);
    assert_eq!(appended.updates[0]["content"]["text"], "append");
    let replacement = tmp.path().join("replacement.jsonl");
    std::fs::write(
        &replacement,
        format!(
            "{}{}",
            fixture_row("c", "newone"),
            fixture_row("d", "newtwo")
        ),
    )
    .expect("replacement");
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        std::fs::metadata(&replacement).unwrap().len()
    );
    std::fs::rename(&replacement, &path).expect("replace");
    changed(&mut rx, sid).await;
    let replaced =
        chat_view_snapshot_blocking(state, sid.into(), Some(appended.epoch), appended.next_seq)
            .await
            .expect("replaced");
    assert!(
        replaced.reset,
        "replacement must invalidate the old conversation cursor"
    );
    assert_eq!(replaced.updates[0]["content"]["text"], "newone");
    assert_eq!(replaced.updates[1]["content"]["text"], "newtwo");
}

// Catches: discovery switching JSONL files while a client retains the old cursor.
#[cfg(unix)]
#[test]
fn replacement_binding_then_appends_do_not_replay_the_old_file() {
    let terminal = DiscoveredTerminal::new();
    let old = terminal.transcript("11111111-1111-4111-8111-111111111111", "old");
    let first = terminal
        .snapshot(None)
        .expect("initial discovery without subagents");
    assert_eq!(first.updates[0]["content"]["text"], "old");
    std::fs::remove_file(old).unwrap();
    let new = terminal.transcript("22222222-2222-4222-8222-222222222222", "new");
    terminal
        .state
        .chat_views
        .views
        .lock()
        .get("discovered")
        .unwrap()
        .lock()
        .bound_at = Instant::now() - REBIND_EVERY;
    let rebound = terminal
        .snapshot(Some(&first))
        .expect("discover replacement");
    assert!(rebound.reset);
    assert_eq!(rebound.updates.len(), 1);
    assert_eq!(rebound.updates[0]["content"]["text"], "new");
    append(&new, &fixture_row("later", "later"));
    let appended = terminal
        .snapshot(Some(&rebound))
        .expect("append after rebind");
    assert!(!appended.reset);
    assert_eq!(appended.updates.len(), 1);
    assert_eq!(appended.updates[0]["content"]["text"], "later");
}

// Catches: a failed ticker read poisoning subsequent consumer snapshots after recovery.
#[cfg(unix)]
#[test]
fn unreadable_transcript_recovers_for_the_snapshot_consumer() {
    let terminal = DiscoveredTerminal::new();
    let uuid = "11111111-1111-4111-8111-111111111111";
    let path = terminal.transcript(uuid, "before");
    let first = terminal.snapshot(None).expect("initial");
    let view = terminal
        .state
        .chat_views
        .views
        .lock()
        .get("discovered")
        .unwrap()
        .clone();
    std::fs::remove_file(&path).unwrap();
    // Exercise ticker cleanup; assertions stay at the consumer boundary.
    tick(&terminal.state, "discovered", &view);
    assert!(
        terminal
            .snapshot(Some(&first))
            .unwrap_err()
            .contains("not_bound")
    );
    terminal.transcript(uuid, "recovered");
    let recovered = terminal.snapshot(Some(&first)).expect("recovered snapshot");
    assert!(recovered.reset);
    assert_eq!(recovered.updates.len(), 1);
    assert_eq!(recovered.updates[0]["content"]["text"], "recovered");
}
