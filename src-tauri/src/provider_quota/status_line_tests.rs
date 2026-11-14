use super::*;
use std::sync::Arc;

const STDIN: &str = include_str!("fixtures/claude-status-line-stdin.json");

fn stdin_for(project: &Path) -> String {
    STDIN.replace("/work/project", &project.to_string_lossy())
}

// Catches: the user status line disappears or changes behind the relay — the
// wrong settings source wins, stdin is altered (trailing newline lost) or the
// command runs without the session's cwd. Drives the generated `claude.json`
// command through the production router on a Unix socket, as Claude runs it.
#[cfg(unix)]
#[tokio::test]
async fn relayed_status_line_output_matches_user_command() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let dir = crate::test_support::short_socket_tempdir();
    let config = dir.path().join("claude config");
    let project = dir.path().join("project");
    let tuic = dir.path().join("tuic");
    std::fs::create_dir_all(project.join(".claude")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.json"),
        r#"{"statusLine":{"type":"command","command":"printf user-level"}}"#,
    )
    .unwrap();
    // Project settings outrank user settings; this is the command Claude runs.
    let user_command = r#"printf 'SL['; cat; printf '] %s' "$PWD""#;
    std::fs::write(
        project.join(".claude/settings.json"),
        serde_json::json!({"statusLine": {"type": "command", "command": user_command, "padding": 2}})
            .to_string(),
    )
    .unwrap();
    crate::agent_hook_launch::regenerate_launch_assets(&tuic).unwrap();
    let settings: Value =
        serde_json::from_slice(&std::fs::read(tuic.join("agent-hooks/claude.json")).unwrap())
            .unwrap();
    assert_eq!(
        settings["statusLine"].as_object().unwrap().len(),
        2,
        "only type and command, so the user's padding and refreshInterval apply"
    );
    let relay = settings["statusLine"]["command"]
        .as_str()
        .unwrap()
        .to_owned();

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    crate::provider_quota::adapters::register(&state.provider_quota);
    let socket = dir.path().join("q.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let router = crate::mcp_http::build_router(state.clone(), false, false);
    tokio::spawn(async move { axum::serve(listener, router).await });

    let stdin = format!("{}\n\n", stdin_for(&project).trim_end());
    let run = |command: String, relayed: bool| {
        let (project, config, socket, stdin) = (
            project.clone(),
            config.clone(),
            socket.clone(),
            stdin.clone(),
        );
        tokio::task::spawn_blocking(move || {
            let mut child = Command::new("/bin/sh");
            child
                .args(["-c", &command])
                .current_dir(&project)
                .env("CLAUDE_CONFIG_DIR", &config)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped());
            if relayed {
                child
                    .env("TUIC_SESSION", "tab-1")
                    .env("TUIC_SOCKET", &socket);
            }
            let mut child = child.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            output.stdout
        })
    };
    let expected = run(user_command.to_owned(), false).await.unwrap();
    let actual = run(relay, true).await.unwrap();
    assert!(
        String::from_utf8_lossy(&expected).contains("}\n\n] "),
        "the fixture must end in newlines, or exact stdin is not tested"
    );
    assert_eq!(
        String::from_utf8_lossy(&actual),
        String::from_utf8_lossy(&expected),
        "the relay must print exactly what the user's status line prints"
    );

    let key = SubscriptionKey::new("claude-code", "anthropic", config.to_string_lossy());
    let view = state.provider_quota.get(&key);
    let snapshot = view.snapshot.expect("relayed windows reach the registry");
    let used: Vec<_> = snapshot.windows.iter().map(|w| w.used_percent).collect();
    assert_eq!(used, [11.0, 29.0]);
    assert_eq!(snapshot.source, QuotaSource::Passive);
}

// Catches: a status line with `refreshInterval` repeats one payload every few
// seconds; stamping each repeat with the arrival time would keep an hours-old
// value fresh forever and suppress the active read.
#[test]
fn repeated_status_line_payload_keeps_its_first_observation_time() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let config = dir.path().to_string_lossy().into_owned();
    let body = stdin_for(dir.path());
    let first = relay(&body, &config, "/home/user", 1_000);
    let key = first.changed.expect("first payload is stored");
    assert!(
        relay(&body, &config, "/home/user", 900_000)
            .changed
            .is_none()
    );
    let owner = crate::provider_quota::adapters::claude_code();
    let observed = owner
        .observed(&key)
        .expect("observed() returns relayed windows");
    assert_eq!(observed.observed_at_ms, 1_000);
    assert_eq!(
        observed.windows[1].resets_at.as_deref(),
        Some("2026-11-15T15:00:00+00:00")
    );
    let changed = body.replace("\"used_percentage\": 11", "\"used_percentage\": 12");
    assert!(
        relay(&changed, &config, "/home/user", 950_000)
            .changed
            .is_some()
    );
    assert_eq!(owner.observed(&key).unwrap().observed_at_ms, 950_000);
}

// Catches: an idle Claude tab on the same account re-sends the rate_limits of its
// last response on every refresh. Stored as the newest value, that stale payload
// rolls the quota view back to a lower percent of the same window, and it flips
// back and forth while several tabs share one account.
#[test]
fn stale_status_line_from_an_idle_session_does_not_roll_the_quota_back() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let config = dir.path().to_string_lossy().into_owned();
    let payload = |five_hour: u32| {
        format!(
            r#"{{"rate_limits":{{"five_hour":{{"used_percentage":{five_hour},"resets_at":1794519600}},"seven_day":{{"used_percentage":29,"resets_at":1794754800}}}}}}"#
        )
    };
    let busy_tab = relay(&payload(60), &config, "/nonexistent-home", 2_000);
    let key = busy_tab.changed.expect("the first payload is stored");
    // The idle tab still holds the value of its last response, same window.
    relay(&payload(40), &config, "/nonexistent-home", 3_000);
    let observed = crate::provider_quota::adapters::claude_code()
        .observed(&key)
        .expect("relayed value");
    assert_eq!(
        observed.windows[0].used_percent, 60.0,
        "a lower percent for the same resets_at is an older reading"
    );
}
