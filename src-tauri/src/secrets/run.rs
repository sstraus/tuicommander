//! The child receives values only in its environment, never argv or a PTY.
use super::*;
use crate::AppState;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    fields: Vec<Field>,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    names: Vec<String>,
    argv: Vec<String>,
    cwd: String,
}

pub(crate) async fn handle_secret(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    tracing::info!(source = "secrets", "Secret tool dispatched");
    let result = match args["action"].as_str() {
        Some("request") => {
            let mut input = args.clone();
            input.as_object_mut().map(|o| o.remove("action"));
            match serde_json::from_value::<Request>(input) {
                Ok(request) => match Form::request(request.fields, request.reason) {
                    Ok(form) => forms::ask(state, form).await.and_then(|s| {
                        serde_json::to_value(s).map_err(|_| "Could not encode secret status".into())
                    }),
                    Err(error) => Err(error),
                },
                Err(_) => Err("Invalid secret request schema".into()),
            }
        }
        Some("run") => {
            let mut input = args.clone();
            input.as_object_mut().map(|o| o.remove("action"));
            match serde_json::from_value::<Run>(input) {
                Ok(run) => execute(state, run).await,
                Err(_) => Err("Invalid secret command".into()),
            }
        }
        Some("remove") => match serde_json::from_value::<Vec<String>>(args["names"].clone()) {
            Ok(names) => {
                state.secrets.remove(&names);
                Ok(serde_json::json!({"names": names, "status": "removed"}))
            }
            Err(_) => Err("Invalid secret names".into()),
        },
        _ => Err("Secret actions: request, run, remove".into()),
    };
    match &result {
        Ok(_) => tracing::info!(source = "secrets", "Secret tool completed"),
        Err(error) => tracing::warn!(source = "secrets", %error, "Secret tool failed"),
    }
    result.unwrap_or_else(|error| serde_json::json!({"error": error}))
}

async fn execute(state: &Arc<AppState>, run: Run) -> Result<serde_json::Value, String> {
    policy::validate_argv(&run.argv)?;
    if run.names.is_empty() || run.names.len() > 16 || run.names.iter().any(|n| !valid_name(n)) {
        return Err("Invalid secret names".into());
    }
    let cwd = std::fs::canonicalize(&run.cwd).map_err(|_| "Invalid working directory")?;
    if !cwd.is_dir() {
        return Err("Working directory is not a directory".into());
    }
    let environment = match state.secrets.environment(&run.names) {
        Ok(env) => env,
        Err(_) => return Ok(serde_json::json!({"names": run.names, "status": "missing"})),
    };
    let program = resolve_program(&run.argv[0])?;
    let mut resolved_argv = run.argv.clone();
    resolved_argv[0] = program.to_string_lossy().into_owned();
    policy::validate_argv(&resolved_argv)?;
    // A template grants precisely this argv. Fixed absolute executable paths
    // prevent a later PATH change from silently selecting a different program.
    let allowed = state
        .secrets
        .allowed(&resolved_argv, &run.names, &cwd.to_string_lossy());
    if !allowed {
        let fields = run
            .names
            .iter()
            .map(|name| Field {
                name: name.clone(),
                kind: FieldKind::Password,
                display: None,
            })
            .collect();
        let mut form = Form::request(
            fields,
            format!("Approve this exact argv in {}", cwd.display()),
        )?;
        form.argv = Some(resolved_argv.clone());
        form.cwd = Some(cwd.to_string_lossy().into_owned());
        let status = forms::ask(state, form).await?;
        if status.status != "approved" {
            return Ok(serde_json::json!({"names": run.names, "status": "declined"}));
        }
    }
    run_child(&resolved_argv, &cwd, &environment).await
}

fn resolve_program(program: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(program);
    if path.is_absolute() {
        return std::fs::canonicalize(path).map_err(|_| "Program was not found".into());
    }
    if program.contains(['/', '\\']) {
        return Err("Program must be an absolute path or executable name".into());
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return Err("PATH is unavailable; use an absolute program path".into());
    };
    for dir in std::env::split_paths(&paths).filter(|p| p.is_absolute()) {
        #[cfg(windows)]
        let candidates = [dir.join(program), dir.join(format!("{program}.exe"))];
        #[cfg(not(windows))]
        let candidates = [dir.join(program)];
        for candidate in candidates {
            if candidate.is_file() {
                return std::fs::canonicalize(candidate)
                    .map_err(|_| "Program was not found".into());
            }
        }
    }
    Err("Program was not found".into())
}

pub(super) async fn run_child(
    argv: &[String],
    cwd: &std::path::Path,
    environment: &BTreeMap<String, Zeroizing<String>>,
) -> Result<serde_json::Value, String> {
    use std::process::Stdio;
    let mut command = tokio::process::Command::new(&argv[0]);
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SystemRoot",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (name, value) in environment {
        command.env(name, value.as_str());
    }
    #[cfg(unix)]
    {
        // Detach from the host controlling terminal: /dev/tty must not bypass
        // the captured stdout/stderr path into a terminal ring or tcap stream.
        unsafe {
            command.pre_exec(|| {
                // SAFETY: setsid is async-signal-safe and uses no Rust locks or allocations.
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = command
        .spawn()
        .map_err(|_| "Secret command could not start")?;
    let stdout = child.stdout.take().ok_or("Missing child stdout")?;
    let stderr = child.stderr.take().ok_or("Missing child stderr")?;
    let capture = async {
        tokio::try_join!(read_capped(stdout), read_capped(stderr), async {
            child
                .wait()
                .await
                .map_err(|_| "Secret command wait failed".to_string())
        })
    };
    let captured = tokio::time::timeout(tuic_ipc::SECRET_CHILD_TIMEOUT, capture).await;
    let (stdout, stderr, status) = match captured {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            return Err(error);
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err("Secret command timed out; output withheld".into());
        }
    };
    let needles: Vec<_> = environment
        .values()
        .flat_map(|value| mask::representations(value))
        .collect();
    Ok(
        serde_json::json!({"status": "completed", "exit_code": status.code(), "stdout": mask::mask_bytes(&stdout, &needles), "stderr": mask::mask_bytes(&stderr, &needles)}),
    )
}

async fn read_capped(
    reader: impl tokio::io::AsyncRead + Unpin,
) -> Result<Zeroizing<Vec<u8>>, String> {
    const CAP: usize = 1024 * 1024;
    let mut bytes = Zeroizing::new(Vec::new());
    reader
        .take((CAP + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "Secret output capture failed")?;
    if bytes.len() > CAP {
        return Err("Secret output limit exceeded; output withheld".into());
    }
    Ok(bytes)
}
