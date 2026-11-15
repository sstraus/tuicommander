//! Detached command runner for managed CLI callers.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::ipc;

const WAKE_ATTEMPTS: u64 = 6;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(200);

#[derive(Debug)]
struct WakeError {
    message: String,
    retryable: bool,
    acceptance_uncertain: bool,
}

impl WakeError {
    fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
            acceptance_uncertain: false,
        }
    }

    fn from_io(error: std::io::Error) -> Self {
        let retryable = matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::NotFound
        );
        Self {
            message: error.to_string(),
            retryable,
            acceptance_uncertain: false,
        }
    }

    fn after_post(mut self) -> Self {
        // No complete receipt: bytes may already have reached the queue.
        self.acceptance_uncertain = true;
        self
    }

    fn from_http(operation: &str, status: u16, body: &str) -> Self {
        Self {
            message: format!("{operation} answered HTTP {status}: {body}"),
            retryable: status >= 500,
            acceptance_uncertain: operation == "Queue" && status >= 500,
        }
    }
}

fn write_wake_status(path: &str, status: &Value) -> Result<(), String> {
    let temp = format!("{path}.tmp");
    fs::write(&temp, format!("{status}\n"))
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|error| format!("Cannot write wake status file {path}: {error}"))
}

fn marker_path(caller: &str) -> Result<std::path::PathBuf, String> {
    if caller.is_empty()
        || caller.len() > 128
        || !caller
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("Invalid TUIC_SESSION for background wake marker".into());
    }
    let directory = if let Some(path) = std::env::var_os("TUIC_BG_WAKE_DIR") {
        std::path::PathBuf::from(path)
    } else {
        ipc::config_dir().join("bg-wakes")
    };
    Ok(directory.join(format!("{caller}.json")))
}

fn write_marker(caller: &str, status: &Value) -> Result<(), String> {
    let path = marker_path(caller)?;
    fs::create_dir_all(path.parent().expect("marker has parent")).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut record = status.clone();
    record["session_id"] = serde_json::json!(caller);
    fs::write(&temp, format!("{record}\n"))
        .and_then(|()| fs::rename(&temp, &path))
        .map_err(|e| {
            format!(
                "Cannot write background wake marker {}: {e}",
                path.display()
            )
        })
}

pub fn launch(log: &str, command: &[String]) -> Result<(), String> {
    crate::bg_process::timeout()?;
    let caller = std::env::var("TUIC_SESSION").unwrap_or_default();
    if caller.is_empty() {
        eprintln!("tuic: bg requires TUIC_SESSION; nobody would be woken");
        std::process::exit(2);
    }
    if command.is_empty() {
        eprintln!("tuic: bg requires a command after --");
        std::process::exit(2);
    }
    if let Some(parent) = Path::new(log)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Cannot create background log directory {}: {e}",
                parent.display()
            )
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("Cannot open background log {log}: {e}"))?;
    let exit_file = format!("{log}.exit");
    if let Err(e) = fs::remove_file(&exit_file)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("Cannot remove stale exit file {exit_file}: {e}"));
    }
    let wake_file = format!("{log}.wake");
    if let Err(e) = fs::remove_file(&wake_file)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("Cannot remove stale wake file {wake_file}: {e}"));
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Host unavailability must not prevent the detached command from running.
    // Without launch-time authority its result is retained, never auto-submitted,
    // unless the host predates wake origins: then it queues as before.
    let session = caller_session(caller.as_str()).ok();
    let origin = session
        .as_ref()
        .and_then(|session| session["state"]["background_wake_origin"].as_str())
        .map(str::to_owned)
        .unwrap_or_default();
    let legacy_host = session.as_ref().is_some_and(predates_wake_origins);
    write_marker(
        &caller,
        &serde_json::json!({"status": "retrying", "attempts": 0}),
    )?;
    let mut runner = Command::new(exe);
    runner.env("TUIC_BG_COMPLETION_ORIGIN", &origin);
    runner.env(LEGACY_HOST_ENV, if legacy_host { "1" } else { "0" });
    #[cfg(unix)]
    runner.env("TUIC_SOCKET", crate::ipc::socket_path());
    if let Some(id) = tuic_ipc::app_instance::current_app_instance().named_id() {
        runner.env(tuic_ipc::app_instance::APP_INSTANCE_ENV_VAR, id);
    }
    runner
        .arg("__bg-runner")
        .arg(log)
        .arg(&caller)
        .arg("--")
        .args(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        runner.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        // Command::spawn inherits every inheritable handle, including the
        // launcher's original capture pipes, even with null runner stdio.
        // This CLI has no concurrent process launches; keep those pipes in
        // this process so its caller observes EOF as soon as launch returns.
        use windows_sys::Win32::Foundation::{
            HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
        };
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        for kind in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: GetStdHandle returns a borrowed process handle; it is
            // inspected and never closed. Clearing inheritance keeps it usable
            // by this process and does not alter runner's explicit null stdio.
            let handle = unsafe { GetStdHandle(kind) };
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                // SAFETY: handle is a live standard handle owned by this CLI.
                if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
                    return Err(format!(
                        "Cannot detach background standard handle: {}",
                        std::io::Error::last_os_error()
                    ));
                }
            }
        }
        runner.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let child = runner.spawn().map_err(|e| {
        let message = format!("Cannot start background runner: {e}");
        let _ = write_marker(
            &caller,
            &serde_json::json!({"status": "failed", "attempts": 0, "error": message}),
        );
        message
    })?;
    println!("tuic bg: pid={} log={log} wake={wake_file}", child.id());
    Ok(())
}

pub fn run(log: &str, caller: &str, command: &[String]) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("Cannot open background log {log}: {e}"))?;
    let full_command = command.join(" ");
    writeln!(output, "tuic bg: command={full_command}")
        .map_err(|e| format!("Cannot write background log header {log}: {e}"))?;
    let stderr = output.try_clone().map_err(|e| e.to_string())?;
    let mut process = Command::new(&command[0]);
    process
        .args(&command[1..])
        // The command is not the foreground peer. A detached reviewer that
        // inherits its identity can report progress blocked as that peer and
        // strand this runner's completion behind its own confident wait.
        // Keep the runner's identity for completion delivery, not in the command tree.
        .env_remove("TUIC_SESSION")
        .env_remove("TUIC_PARENT")
        .env_remove("TUIC_BG_COMPLETION_ORIGIN")
        .env_remove(LEGACY_HOST_ENV)
        // Stop headerless MCP ancestry recovery at this detached command.
        .env("TUIC_BG_COMMAND", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(stderr));
    let code = match crate::bg_process::run(&mut process, crate::bg_process::timeout()?) {
        Ok((code, true)) => {
            writeln!(
                output,
                "tuic bg: background deadline exceeded; owned command stopped"
            )
            .map_err(|e| e.to_string())?;
            code
        }
        Ok((code, false)) => code,
        Err(e) => {
            let _ = writeln!(output, "tuic bg: command failed: {e}");
            127
        }
    };
    let exit_file = format!("{log}.exit");
    fs::write(&exit_file, format!("{code}\n"))
        .map_err(|e| format!("Cannot write exit file {exit_file}: {e}"))?;
    let first_line = full_command.split(['\r', '\n']).next().unwrap_or_default();
    let mut label: String = first_line.chars().take(120).collect();
    if first_line.len() < full_command.len() || first_line.chars().nth(120).is_some() {
        label = label.chars().take(119).collect();
        label.push('…');
    }
    let wake = format!("BG DONE exit={code} log={log} cmd={label}");
    let wake_file = format!("{log}.wake");
    // No mail id exists before enqueue. One runner identity distinguishes jobs
    // even when they reuse the same log path and command, and survives retries.
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let key = format!("bg-{}-{started}", std::process::id());
    let mut acceptance_uncertain = false;
    for attempt in 1..=WAKE_ATTEMPTS {
        let origin = std::env::var("TUIC_BG_COMPLETION_ORIGIN").unwrap_or_default();
        let legacy_host = std::env::var(LEGACY_HOST_ENV).is_ok_and(|value| value == "1");
        let wake_status = match queue_wake(caller, &wake, &key, &origin, legacy_host) {
            Ok(retained) => {
                serde_json::json!({"status": if retained { "retained" } else { "queued" }, "tuic_session": caller, "attempts": attempt, "completion": wake})
            }
            Err(queue_error) => {
                acceptance_uncertain |= queue_error.acceptance_uncertain;
                let _ = writeln!(
                    output,
                    "tuic bg: queue wake attempt {attempt} failed: {}",
                    queue_error.message
                );
                // Mail is not a second wake transport: it cannot carry the
                // launch capability through the queue's stop/delivery gate.
                if queue_error.retryable && attempt < WAKE_ATTEMPTS {
                    let status = serde_json::json!({"status": "retrying", "tuic_session": caller, "attempts": attempt, "error": queue_error.message, "completion": wake});
                    write_marker(caller, &status)?;
                    write_wake_status(&wake_file, &status)?;
                    sleep_before_wake_retry(attempt, std::thread::sleep);
                    continue;
                }
                serde_json::json!({"status": if acceptance_uncertain { "uncertain" } else { "retained" }, "tuic_session": caller, "attempts": attempt, "error": queue_error.message, "completion": wake})
            }
        };
        write_marker(caller, &wake_status)?;
        write_wake_status(&wake_file, &wake_status).map_err(|message| {
            let _ = writeln!(output, "tuic bg: {message}");
            message
        })?;
        break;
    }
    Ok(())
}

// The retry schedule is private; accepting the sleeper keeps tests off wall-clock time.
fn sleep_before_wake_retry(attempt: u64, sleep: impl FnOnce(Duration)) {
    sleep(INITIAL_RETRY_DELAY * (1 << (attempt - 1)));
}

fn caller_session(caller: &str) -> Result<Value, WakeError> {
    let sessions = ipc::get("/sessions").map_err(WakeError::from_io)?;
    if !sessions.is_success() {
        return Err(WakeError::from_http(
            "Session lookup",
            sessions.status,
            &sessions.body,
        ));
    }
    let rows: Value = sessions
        .json()
        .map_err(|e| WakeError::permanent(e.to_string()))?;
    let matches: Vec<&Value> = rows
        .as_array()
        .ok_or_else(|| WakeError::permanent("Session lookup did not return a list"))?
        .iter()
        .filter(|row| row["tuic_session"].as_str() == Some(caller))
        .collect();
    let [session] = matches.as_slice() else {
        return Err(WakeError::permanent(format!(
            "Expected one live session for {caller}, found {}",
            matches.len()
        )));
    };
    Ok((*session).clone())
}

// DEFERRED (2026-11-13) — compatibility path for a host that predates
// `background_wake_origin` (the CLI lands before the app restarts). Remove it
// with the next host contract change.
const LEGACY_HOST_ENV: &str = "TUIC_BG_LEGACY_HOST_WAKE";

/// A state object without the field comes from an old host. A missing state is
/// not proof of an old host: a current host cannot accept that wake either.
fn predates_wake_origins(session: &Value) -> bool {
    session["state"]
        .as_object()
        .is_some_and(|state| !state.contains_key("background_wake_origin"))
}

fn queue_wake(
    caller: &str,
    wake: &str,
    key: &str,
    origin: &str,
    legacy_host: bool,
) -> Result<bool, WakeError> {
    if origin.is_empty() && !legacy_host {
        return Ok(true); // An unbound legacy runner retains evidence, never wakes.
    }
    let session = caller_session(caller)?;
    if origin.is_empty() && !predates_wake_origins(&session) {
        return Ok(true); // The host restarted into one that revokes wakes.
    }
    let session_id = session["session_id"]
        .as_str()
        .ok_or_else(|| WakeError::permanent("Session has no PTY identity"))?;
    let mut body = serde_json::json!({"text": wake, "idempotencyKey": key});
    if !origin.is_empty() {
        body["automaticCompletionOrigin"] = origin.into();
    }
    let body = body.to_string();
    let response = ipc::post(&format!("/sessions/{session_id}/queue"), &body)
        .map_err(|e| WakeError::from_io(e).after_post())?;
    if !response.is_success() {
        return Err(WakeError::from_http(
            "Queue",
            response.status,
            &response.body,
        ));
    }
    let receipt: Value = response
        .json()
        .map_err(|e| WakeError::permanent(e.to_string()).after_post())?;
    if receipt["accepted"] != true
        && receipt["typed"] != true
        && receipt["queued"].as_u64().is_none_or(|count| count == 0)
    {
        return Err(WakeError::permanent(format!(
            "Queue did not accept wake: {}",
            response.body
        )));
    }
    Ok(receipt["retained"] == true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: an unbound runner falls back to ordinary queue/mail authority.
    #[test]
    fn missing_completion_origin_retains_evidence_without_contacting_a_wake_transport() {
        assert!(queue_wake("not-a-live-session", "retained-result", "job", "", false).unwrap());
    }

    // Catches: a current-host row without state (no accumulator entry) is read as an
    // old host, so the CLI queues an unrevocable wake that an Escape cannot stop.
    #[test]
    fn only_a_state_without_the_origin_field_marks_an_old_host() {
        let row = |state: Value| serde_json::json!({"session_id": "pty-1", "state": state});
        assert!(predates_wake_origins(&row(serde_json::json!({}))));
        assert!(!predates_wake_origins(&row(Value::Null)));
        assert!(!predates_wake_origins(
            &serde_json::json!({"session_id": "pty-1"})
        ));
        assert!(!predates_wake_origins(&row(
            serde_json::json!({"background_wake_origin": "o"})
        )));
    }

    // Catches: division, reversed shifts or an off-by-one exponent collapses the retry backoff.
    #[test]
    fn failed_wake_attempts_double_the_backoff_before_each_of_five_retries() {
        assert_eq!(WAKE_ATTEMPTS, 6);
        let mut delays = Vec::new();
        for attempt in 1..WAKE_ATTEMPTS {
            sleep_before_wake_retry(attempt, |delay| delays.push(delay.as_millis()));
        }
        assert_eq!(delays, [200, 400, 800, 1600, 3200]);
    }

    // Catches: inverted length/character guards permit path traversal or reject boundary IDs.
    #[test]
    fn marker_path_accepts_safe_boundary_ids_and_refuses_unsafe_names() {
        for caller in [
            "a".to_string(),
            "A0-_".to_string(),
            "a".repeat(127),
            "a".repeat(128),
        ] {
            let path = marker_path(&caller).expect("safe caller ID");
            assert_eq!(
                path.file_name().unwrap().to_str().unwrap(),
                format!("{caller}.json")
            );
        }
        for caller in [
            String::new(),
            "a".repeat(129),
            "../escape".into(),
            "a/b".into(),
            "a\\b".into(),
            "..".into(),
            ".".into(),
            "a.b".into(),
            "a b".into(),
            "a\n".into(),
            "é".into(),
        ] {
            assert_eq!(
                marker_path(&caller).unwrap_err(),
                "Invalid TUIC_SESSION for background wake marker",
                "{caller:?}"
            );
        }
    }
}
