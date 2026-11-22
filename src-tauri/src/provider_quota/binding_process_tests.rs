//! Exercise the scheduler against real child process environments.
#![cfg(unix)]
use super::*;
use crate::state::{PtySession, SessionState, tests_support::make_test_app_state};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::sync::atomic::AtomicBool;

fn spawn_process(state: &crate::AppState, id: &str, config: &str) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let sleep = crate::test_support::env_readable_system_binary("sleep");
    let mut command = CommandBuilder::new(&sleep);
    command.arg("30");
    command.env("CLAUDE_CONFIG_DIR", config);
    command.env_remove("ANTHROPIC_API_KEY");
    command.env_remove("ANTHROPIC_AUTH_TOKEN");
    let child = pair.slave.spawn_command(command).unwrap();
    let master = pair.master;
    let writer = master.take_writer().unwrap();
    state.session_maps.sessions.insert(
        id.into(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: sleep.to_string_lossy().into_owned(),
        }),
    );
    let session = SessionState {
        agent_type: Some("claude".into()),
        ..Default::default()
    };
    state.session_maps.session_states.insert(id.into(), session);
}

/// Setup, not behaviour: wait with no deadline until the session's foreground
/// process exposes `variable` in the environment the scheduler reads. A loaded
/// box only makes this slower; nextest `terminate-after` bounds a real hang.
async fn wait_for_readable_environment(state: &crate::AppState, id: &str, variable: &str) {
    loop {
        let ready = crate::pty::session_leaf_pid(state, id).is_some_and(|pid| {
            crate::process_env::read_process_environment(pid)
                .is_ok_and(|env| env.iter().any(|entry| entry == variable))
        });
        if ready {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Behaviour: start the scheduler and wait for every session to be bound.
/// The scheduler passes once at start and then once per `tick`, so a live
/// session is bound within one tick or never; the bound is that tick, not a
/// guess at how fast a loaded box reads process environments.
async fn bindings_after_first_pass(
    state: &Arc<crate::AppState>,
    ids: &[&str],
) -> Result<Vec<SubscriptionKey>, String> {
    let bound = state.provider_quota.config().tick;
    spawn_scheduler(state.clone());
    let result = tokio::time::timeout(bound, async {
        loop {
            let keys: Option<Vec<_>> = ids
                .iter()
                .map(|id| state.provider_quota.binding(id))
                .collect();
            if let Some(keys) = keys {
                break keys;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    result.map_err(|_| {
        let unbound: Vec<_> = ids
            .iter()
            .filter(|id| state.provider_quota.binding(id).is_none())
            .collect();
        format!("scheduler pass did not bind {unbound:?} within one tick ({bound:?})")
    })
}

/// Catches: both terminals inherit a single host/default account instead of
/// resolving the credentials of their separate live foreground processes.
#[tokio::test]
async fn live_processes_with_different_config_dirs_get_distinct_bindings() {
    let state = Arc::new(make_test_app_state());
    let sessions = [
        ("first", "/quota-test/first"),
        ("second", "/quota-test/second"),
        ("same", "/quota-test/first"),
    ];
    for (id, config) in sessions {
        spawn_process(&state, id, config);
    }
    for (id, config) in sessions {
        wait_for_readable_environment(&state, id, &format!("CLAUDE_CONFIG_DIR={config}")).await;
    }
    let result = bindings_after_first_pass(&state, &["first", "second", "same"]).await;
    for (id, _) in sessions {
        if let Some((_, session)) = state.session_maps.sessions.remove(id) {
            let mut session = session.lock();
            let _ = session._child.kill();
            let _ = session._child.wait();
        }
    }
    let [first, second, same] =
        <[SubscriptionKey; 3]>::try_from(result.unwrap_or_else(|e| panic!("{e}")))
            .expect("one binding per session");
    assert_eq!(first.credential, "/quota-test/first");
    assert_eq!(second.credential, "/quota-test/second");
    assert_ne!(first, second);
    assert_eq!(first, same);
    assert_eq!(state.provider_quota.bound_keys().len(), 2);
}

/// Catches: a Codex that TUIC launched (with its own `-c` overrides) never
/// binding, so its tab shows no usage (3069).
#[tokio::test]
async fn codex_launched_by_tuic_binds_its_codex_home() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let program = dir.path().join("codex");
    std::fs::write(
        &program,
        format!(
            "#!{}\nread input\n",
            crate::test_support::env_readable_system_binary("bash").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let codex_home = dir.path().join("codex-home");
    let state = Arc::new(make_test_app_state());
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(&program);
    command.args([
        "-c",
        r#"projects={"/work/repo"={trust_level="trusted"}}"#,
        "-c",
        "check_for_update_on_startup=false",
        "-c",
        "developer_instructions=\"\"\"\n## TUIC output markers\n\"\"\"",
    ]);
    command.env("CODEX_HOME", &codex_home);
    let child = pair.slave.spawn_command(command).unwrap();
    let master = pair.master;
    let writer = master.take_writer().unwrap();
    state.session_maps.sessions.insert(
        "codex".into(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: true,
            shell: program.to_string_lossy().into_owned(),
        }),
    );
    state.session_maps.session_states.insert(
        "codex".into(),
        SessionState {
            agent_type: Some("codex".into()),
            ..Default::default()
        },
    );
    wait_for_readable_environment(
        &state,
        "codex",
        &format!("CODEX_HOME={}", codex_home.to_string_lossy()),
    )
    .await;
    let result = bindings_after_first_pass(&state, &["codex"]).await;
    if let Some((_, session)) = state.session_maps.sessions.remove("codex") {
        let mut session = session.lock();
        let _ = session._child.kill();
        let _ = session._child.wait();
    }
    let key = result.unwrap_or_else(|e| panic!("{e}")).remove(0);
    assert_eq!(
        key,
        SubscriptionKey::new("codex-cli", "openai-codex", codex_home.to_string_lossy())
    );
}

/// Catches: an ego-shaped foreground process binds to an unrelated CLI login.
#[tokio::test]
async fn ego_process_never_binds_to_cli_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let program = dir.path().join("ego");
    std::fs::write(
        &program,
        format!(
            "#!{}\nread input\n",
            crate::test_support::env_readable_system_binary("bash").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let ego_home = dir.path().join("ego-home");
    let session_dir = ego_home.join("sessions/01a02491-6a90-7ae2-9089-fd6a8532eb82");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("manifest.json"),
        r#"{"model":"openai-codex/fixture"}"#,
    )
    .unwrap();
    let state = Arc::new(make_test_app_state());
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(&program);
    command.args(["--profile", "work"]);
    command.env("EGO_HOME", &ego_home);
    command.env("CLAUDE_CONFIG_DIR", dir.path().join("other-claude"));
    command.env("CODEX_HOME", dir.path().join("other-codex"));
    let child = pair.slave.spawn_command(command).unwrap();
    let pid = child.process_id().unwrap();
    std::fs::write(
        session_dir.join("lease"),
        format!(r#"{{"pid":{pid},"heartbeat":1}}"#),
    )
    .unwrap();
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(session_dir.join("lease"))
        .unwrap();
    lease.lock().unwrap();
    let master = pair.master;
    let writer = master.take_writer().unwrap();
    state.session_maps.sessions.insert(
        "ego".into(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: program.to_string_lossy().into_owned(),
        }),
    );
    let session = SessionState {
        agent_type: Some("ego".into()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert("ego".into(), session);
    wait_for_readable_environment(
        &state,
        "ego",
        &format!("EGO_HOME={}", ego_home.to_string_lossy()),
    )
    .await;
    // Pin the quota reader to an ego without `auth usage`, so an ego installed
    // on the host cannot answer for this throwaway EGO_HOME.
    let no_usage = dir.path().join("ego-without-usage");
    std::fs::write(&no_usage, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&no_usage, std::fs::Permissions::from_mode(0o700)).unwrap();
    state
        .provider_quota
        .register_owner(Arc::new(super::ego::Ego::with_program(no_usage)));
    let result = bindings_after_first_pass(&state, &["ego"]).await;
    if let Some((_, session)) = state.session_maps.sessions.remove("ego") {
        let mut session = session.lock();
        let _ = session._child.kill();
        let _ = session._child.wait();
    }
    let key = result.unwrap_or_else(|e| panic!("{e}")).remove(0);
    assert_eq!(
        key,
        SubscriptionKey::new("ego", "openai-codex", ego_home.to_string_lossy())
    );
    let view = state.provider_quota.ensure(&key).await;
    assert!(
        matches!(view.state, QuotaState::Unavailable { .. }),
        "{:?}",
        view.state
    );
    assert!(view.snapshot.is_none());
}

/// Pin the PTY report to an OS-unreadable process without mocking the process reader.
struct UnreadableProcessMaster(Box<dyn portable_pty::MasterPty + Send>);
impl portable_pty::MasterPty for UnreadableProcessMaster {
    fn resize(&self, size: PtySize) -> Result<(), anyhow::Error> {
        self.0.resize(size)
    }
    fn get_size(&self) -> Result<PtySize, anyhow::Error> {
        self.0.get_size()
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>, anyhow::Error> {
        self.0.try_clone_reader()
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>, anyhow::Error> {
        self.0.take_writer()
    }
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        Some(0)
    }
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        self.0.as_raw_fd()
    }
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        self.0.tty_name()
    }
}

/// Catches: reconcile silently dropping an unreadable environment, or warning every tick.
#[tokio::test]
async fn reconcile_logs_unreadable_process_environment_once() {
    use std::io::Write;
    #[derive(Clone)]
    struct LogSink(Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for LogSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    // A real OS read establishes the failure condition before exercising reconcile.
    assert!(crate::process_env::read_process_environment(0).is_err());
    let state = Arc::new(make_test_app_state());
    let id = "reconcile-unreadable-environment";
    spawn_process(&state, id, "/quota-test/unreadable");
    let (_, session) = state.session_maps.sessions.remove(id).unwrap();
    let mut session = session.into_inner();
    session._child.kill().unwrap();
    session._child.wait().unwrap();
    session.master = Box::new(UnreadableProcessMaster(session.master));
    state
        .session_maps
        .sessions
        .insert(id.into(), Mutex::new(session));
    assert_eq!(crate::pty::session_leaf_pid(&state, id), Some(0));

    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(LogSink(log.clone()))
        .with_ansi(false)
        .finish();
    // reconcile reads processes on spawn_blocking, so a thread-local dispatcher cannot
    // observe its warnings. This is the only global subscriber in the library tests.
    tracing::subscriber::set_global_default(subscriber).unwrap();
    for _ in 0..3 {
        super::binding::reconcile(state.clone()).await;
    }
    assert_eq!(state.provider_quota.binding(id), None);
    let log = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    let warnings: Vec<_> = log.lines().filter(|line| line.contains(id)).collect();
    assert_eq!(warnings.len(), 1, "{log}");
    assert!(warnings[0].contains("Quota binding unavailable"), "{log}");
    assert!(
        warnings[0].contains("process environment unreadable"),
        "{log}"
    );
}
