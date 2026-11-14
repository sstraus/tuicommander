//! Fake-op tests: no test calls the real 1Password CLI.
use super::provider::*;
use super::provider_op::*;
use super::*;
use std::time::Duration;

const FAKE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/secrets/fixtures/fake-op.sh"
);
const VALUE: &str = "s3cr3t-value-2914";

fn op(timeout_ms: u64) -> OpProvider<CommandOpRunner> {
    OpProvider(CommandOpRunner::new(
        "sh",
        &[FAKE],
        Duration::from_millis(timeout_ms),
    ))
}
fn op_ref(item: &str) -> SecretRef {
    SecretRef::Op {
        reference: format!("op://v/i/{item}"),
        account: None,
    }
}

#[test]
fn op_success_returns_value_without_newline() {
    // catches: a trailing newline from `op read` ending up in the secret
    assert_eq!(&*op(5000).resolve(&op_ref("ok")).unwrap(), VALUE);
}

#[test]
fn op_failures_are_typed_and_never_fall_back() {
    // catches: a failed op read stored or substituted as a plain value
    let p = op(5000);
    for (item, want) in [
        ("missing", ProviderError::UnknownReference),
        ("signedout", ProviderError::NotSignedIn),
        ("locked", ProviderError::Locked),
        ("empty", ProviderError::Invalid),
        ("weird", ProviderError::Failed),
    ] {
        assert_eq!(p.resolve(&op_ref(item)).unwrap_err(), want, "{item}");
    }
}

#[test]
fn op_binary_missing_is_op_missing() {
    // catches: a missing CLI reported as a generic failure
    let p = OpProvider(CommandOpRunner::new(
        "/nonexistent/op",
        &[],
        Duration::from_secs(1),
    ));
    assert_eq!(
        p.resolve(&op_ref("ok")).unwrap_err(),
        ProviderError::OpMissing
    );
}

#[test]
fn op_unanswered_prompt_times_out_and_kills_the_child() {
    // catches: a user-away biometric prompt hanging the spawn forever
    let started = std::time::Instant::now();
    assert_eq!(
        op(300).resolve(&op_ref("hang")).unwrap_err(),
        ProviderError::Timeout
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn op_exit_is_not_held_open_by_a_background_helper() {
    // catches: `op` leaving a daemon that inherits stdout, so the pipe read after
    // exit blocks past the timeout and the secret call hangs
    let started = std::time::Instant::now();
    let _ = op(500).resolve(&op_ref("daemon"));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "resolve took {:?} for a 500ms timeout",
        started.elapsed()
    );
}

#[test]
fn op_rejects_flag_like_input_before_spawning() {
    // catches: an agent-supplied account or reference smuggling a CLI flag
    let p = op(5000);
    let bad_account = SecretRef::Op {
        reference: "op://v/i/ok".into(),
        account: Some("--config=/tmp/x".into()),
    };
    let bad_ref = SecretRef::Op {
        reference: "--help".into(),
        account: None,
    };
    assert_eq!(p.resolve(&bad_account).unwrap_err(), ProviderError::Invalid);
    assert_eq!(p.resolve(&bad_ref).unwrap_err(), ProviderError::Invalid);
    let good = SecretRef::Op {
        reference: "op://v/i/args".into(),
        account: Some("my.team@example.com".into()),
    };
    assert_eq!(
        &*p.resolve(&good).unwrap(),
        "read --no-newline --account my.team@example.com op://v/i/args"
    );
}

#[derive(Clone)]
struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Capture {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Catches: an op reference resolved at put time reaching the agent child's env
// through the real spawn path, while the value or the reference leaks into
// logs, session output or any file under the config dir.
#[cfg(unix)]
#[tokio::test]
async fn op_reference_reaches_only_the_spawned_child_env() {
    let logs = Capture(Default::default());
    let writer = logs.clone();
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let config = root.path().join("config");
    let _config = crate::config::set_config_dir_override(config.clone());
    let providers = op(5000);

    let put = |item: &str| {
        serde_json::json!({
            "action": "put", "scope": "task-2914", "ttl": 60,
            "references": [{"name": "API_TOKEN", "op": format!("op://v/i/{item}")}]
        })
    };
    // A failed resolve stores no scope and its reply names no value.
    let failed = run::put_references(&state, &put("locked"), &providers).unwrap_err();
    assert_eq!(failed, ProviderError::Locked.to_string());
    assert!(state.secrets.scope_environment("task-2914").is_err());
    let stored = run::put_references(&state, &put("ok"), &providers).unwrap();
    assert_eq!(
        stored,
        serde_json::json!({"names": ["API_TOKEN"], "status": "stored"})
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::mcp_http::build_router(state.clone(), false, true);
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let base = format!("http://{address}");
    let reply: serde_json::Value = client
        .post(format!("{base}/sessions/agent"))
        .json(&serde_json::json!({
            "binary_path": "/bin/sh",
            "args": ["-c", "read -r line; printf 'echo!%s!\\n' \"$API_TOKEN\"; printf '%s' \"$API_TOKEN\" > \"$OUT\"; read -r line"],
            "prompt": "not a secret", "scope": "task-2914",
            "env": {"OUT": root.path().join("child-env").to_str().unwrap()}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = reply["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("spawn failed: {reply}"))
        .to_owned();
    client
        .post(format!("{base}/sessions/{id}/write"))
        .json(&serde_json::json!({"data": "go\n"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let out_file = root.path().join("child-env");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !out_file.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let seen = std::fs::read_to_string(&out_file).expect("child never wrote its env value");
    // The masker withholds a short tail; wait for the mask itself, so the
    // no-leak assertion below cannot pass on output that never arrived.
    let output = loop {
        let body = client
            .get(format!("{base}/sessions/{id}/output?format=raw"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        if body.contains("echo!****") || std::time::Instant::now() > deadline {
            break body;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    if let Some(entry) = state.session_maps.sessions.get(&id) {
        let _ = entry.lock()._child.kill();
    }
    crate::pty::cleanup_session(&id, &state);
    server.abort();

    assert_eq!(seen, VALUE, "the child env must hold the resolved value");
    assert!(
        output.contains("echo!****"),
        "child echo missing or unmasked: {output}"
    );
    assert!(!output.contains(VALUE), "value in session output: {output}");
    assert!(!reply.to_string().contains(VALUE));
    let captured = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(!captured.contains(VALUE), "value in logs");
    assert!(!captured.contains("op://"), "reference in logs");
    // Stored config: nothing under the config dir carries the value or the reference.
    for entry in walkdir_files(&config) {
        let text = String::from_utf8_lossy(&std::fs::read(&entry).unwrap()).into_owned();
        assert!(
            !text.contains(VALUE) && !text.contains("op://"),
            "{entry:?}"
        );
    }
}

fn walkdir_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walkdir_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}
