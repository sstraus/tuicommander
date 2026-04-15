use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Environment variables that the child process is allowed to inherit from the
/// Tauri parent. Everything else (notably ANTHROPIC_API_KEY, GITHUB_TOKEN,
/// OPENAI_API_KEY, AWS_*, …) is stripped so that user-authored shell/headless
/// prompt scripts cannot exfiltrate host secrets by simply echoing env vars.
///
/// Callers remain free to inject additional vars via the `env` parameter; those
/// are applied on top of the allowlist (see [`apply_clean_env`]).
const ENV_ALLOWLIST: &[&str] = &[
    // POSIX essentials
    "PATH", "HOME", "SHELL", "TERM", "USER", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE",
    // Windows equivalents
    "TMP", "TEMP", "USERPROFILE", "USERNAME", "SYSTEMROOT", "COMSPEC",
];

/// Clear the child's env, re-populate from the allowlist inherited from the
/// current process, then overlay caller-supplied vars. Centralises the policy
/// so headless and shell paths can't drift out of sync.
fn apply_clean_env(cmd: &mut Command, extra: Option<&HashMap<String, String>>) {
    cmd.env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
    if let Some(vars) = extra {
        for (k, v) in vars {
            cmd.env(k, v);
        }
    }
}

/// Execute a headless (one-shot) agent command and capture stdout.
///
/// `command` is the binary to run and `args` are literal argv elements — no shell
/// interpolation is performed, so characters like `;`, `&&`, backticks or `$()` in
/// args are passed to the child process verbatim. `stdin_content` is piped to the
/// process stdin to convey prompt content. Timeout is capped at 5 minutes.
#[tauri::command]
pub(crate) async fn execute_headless_prompt(
    command: String,
    args: Vec<String>,
    stdin_content: Option<String>,
    timeout_ms: u64,
    repo_path: String,
    env: Option<std::collections::HashMap<String, String>>,
) -> Result<String, String> {
    let duration = Duration::from_millis(timeout_ms.min(300_000)); // Cap at 5 minutes

    if command.trim().is_empty() {
        return Err("command must not be empty".into());
    }

    let needs_stdin = stdin_content.is_some();
    let mut cmd = Command::new(&command);
    cmd.args(&args)
        .current_dir(&repo_path)
        .stdin(if needs_stdin { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Clear inherited env and inject only allowlist + caller-supplied vars.
    // See ENV_ALLOWLIST for the rationale.
    apply_clean_env(&mut cmd, env.as_ref());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    // Pipe prompt content via stdin to avoid shell injection
    if let Some(content) = stdin_content
        && let Some(mut stdin) = child.stdin.take()
    {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(content.as_bytes()).await;
        drop(stdin); // Close stdin so the process can proceed
    }

    match timeout(duration, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(if stderr.is_empty() {
                    format!(
                        "Process exited with code {}",
                        output.status.code().unwrap_or(-1)
                    )
                } else {
                    stderr
                })
            }
        }
        Ok(Err(e)) => Err(format!("Process error: {e}")),
        Err(_) => Err(format!("Timed out after {}s", timeout_ms / 1000)),
    }
}

/// Execute a shell script directly and capture stdout.
///
/// Unlike `execute_headless_prompt` (which runs a CLI agent command and pipes content
/// via stdin), this executes `script_content` itself as a shell script — no agent involved.
/// Timeout is capped at 60 seconds.
#[tauri::command]
pub(crate) async fn execute_shell_script(
    script_content: String,
    timeout_ms: u64,
    repo_path: String,
) -> Result<String, String> {
    let duration = Duration::from_millis(timeout_ms.min(60_000)); // Cap at 60 seconds

    let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
    let shell_flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_flag)
        .arg(&script_content)
        .current_dir(&repo_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Shell scripts never receive caller-supplied env vars today — strip the
    // inherited parent env to the allowlist so repo-controlled script_content
    // cannot read ANTHROPIC_API_KEY / GITHUB_TOKEN / etc. from the Tauri process.
    apply_clean_env(&mut cmd, None);

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;

    match timeout(duration, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(if stderr.is_empty() {
                    format!(
                        "Process exited with code {}",
                        output.status.code().unwrap_or(-1)
                    )
                } else {
                    stderr
                })
            }
        }
        Ok(Err(e)) => Err(format!("Process error: {e}")),
        Err(_) => Err(format!("Timed out after {}s", timeout_ms / 1000)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn headless_echo_command() {
        let result = execute_headless_prompt(
            "echo".to_string(),
            vec!["hello".to_string()],
            None,
            5000,
            "/tmp".to_string(),
            None,
        ).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn headless_stdin_piped() {
        let result = execute_headless_prompt(
            "cat".to_string(),
            vec![],
            Some("hello from stdin".to_string()),
            5000,
            "/tmp".to_string(),
            None,
        ).await;
        assert_eq!(result.unwrap(), "hello from stdin");
    }

    #[tokio::test]
    async fn headless_nonzero_exit() {
        let result = execute_headless_prompt(
            "false".to_string(),
            vec![],
            None,
            5000,
            "/tmp".to_string(),
            None,
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn headless_timeout() {
        let result = execute_headless_prompt(
            "sleep".to_string(),
            vec!["10".to_string()],
            None,
            100,
            "/tmp".to_string(),
            None,
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Timed out"));
    }

    #[tokio::test]
    async fn headless_env_vars_injected() {
        let env = Some([("TUIC_TEST_VAR".to_string(), "injected_value".to_string())].into_iter().collect());
        // printenv reads env directly — no shell interpolation needed.
        let result = execute_headless_prompt(
            "printenv".to_string(),
            vec!["TUIC_TEST_VAR".to_string()],
            None,
            5000,
            "/tmp".to_string(),
            env,
        ).await;
        assert_eq!(result.unwrap(), "injected_value");
    }

    /// Run config with shell metacharacters in args must be passed literally —
    /// no command injection regardless of arg content.
    #[tokio::test]
    async fn headless_args_shell_metachars_are_literal() {
        // Semicolon + command substitution + backticks — if passed through a shell,
        // these would execute `whoami` / run `rm -rf`. With argv form, echo prints them verbatim.
        let injection = "safe; rm -rf /tmp/tuictest_inject; $(whoami); `whoami`".to_string();
        let result = execute_headless_prompt(
            "echo".to_string(),
            vec![injection.clone()],
            None,
            5000,
            "/tmp".to_string(),
            None,
        ).await;
        assert_eq!(result.unwrap(), injection);
        // Confirm the would-be created file does not exist.
        assert!(!std::path::Path::new("/tmp/tuictest_inject").exists());
    }

    #[tokio::test]
    async fn headless_empty_command_rejected() {
        let result = execute_headless_prompt(
            "   ".to_string(),
            vec![],
            None,
            5000,
            "/tmp".to_string(),
            None,
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("command must not be empty"));
    }

    #[tokio::test]
    async fn shell_script_echo() {
        let result =
            execute_shell_script("echo hello".to_string(), 5000, "/tmp".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn shell_script_multiline() {
        let result = execute_shell_script(
            "echo line1\necho line2".to_string(),
            5000,
            "/tmp".to_string(),
        ).await;
        assert_eq!(result.unwrap(), "line1\nline2");
    }

    #[tokio::test]
    async fn shell_script_nonzero_exit() {
        let result =
            execute_shell_script("exit 1".to_string(), 5000, "/tmp".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn shell_script_timeout() {
        let result =
            execute_shell_script("sleep 10".to_string(), 100, "/tmp".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Timed out"));
    }

    #[tokio::test]
    async fn shell_script_uses_cwd() {
        let result =
            execute_shell_script("pwd".to_string(), 5000, "/tmp".to_string()).await;
        // macOS resolves /tmp → /private/tmp
        assert!(result.unwrap().contains("tmp"));
    }

    /// Setting sensitive secrets on the parent and then spawning a headless
    /// child must NOT leak them into the child's environment. Previously
    /// Command was spawned without env_clear(), so anything exported by the
    /// Tauri process (ANTHROPIC_API_KEY, GITHUB_TOKEN, …) became available
    /// to user-authored headless prompt scripts. Story 1272-c98c.
    #[tokio::test]
    async fn headless_does_not_leak_parent_secret_envs() {
        // SAFETY: these tests run single-threaded per tokio-test file and only
        // mutate env for the duration of the assert. Still, use unique keys
        // to reduce collision risk with anything else.
        const LEAK_KEYS: &[&str] = &[
            "TUIC_LEAK_TEST_ANTHROPIC_API_KEY",
            "TUIC_LEAK_TEST_GITHUB_TOKEN",
            "TUIC_LEAK_TEST_OPENAI_API_KEY",
        ];
        for k in LEAK_KEYS {
            // SAFETY: test-only env mutation; acceptable within a scoped test.
            unsafe { std::env::set_var(k, "SHOULD-NOT-LEAK") };
        }

        // Spawn without passing the key in the `env` map: it must NOT appear.
        for k in LEAK_KEYS {
            let out = execute_headless_prompt(
                "sh".to_string(),
                vec!["-c".to_string(), format!("echo \"${{{k}:-UNSET}}\"")],
                None,
                5000,
                "/tmp".to_string(),
                None,
            )
            .await
            .unwrap();
            assert_eq!(out, "UNSET", "leaked {k} into child env");
        }

        for k in LEAK_KEYS {
            // SAFETY: see above.
            unsafe { std::env::remove_var(k) };
        }
    }

    #[tokio::test]
    async fn shell_script_does_not_leak_parent_secret_envs() {
        const LEAK_KEY: &str = "TUIC_LEAK_TEST_SHELL_SECRET";
        // SAFETY: test-only env mutation.
        unsafe { std::env::set_var(LEAK_KEY, "SHOULD-NOT-LEAK") };

        let out = execute_shell_script(
            format!("echo \"${{{LEAK_KEY}:-UNSET}}\""),
            5000,
            "/tmp".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(out, "UNSET");

        // SAFETY: see above.
        unsafe { std::env::remove_var(LEAK_KEY) };
    }

    #[tokio::test]
    async fn headless_allowlist_keeps_path() {
        // PATH is on the allowlist, so the child must still be able to resolve
        // common binaries — otherwise `sh -c 'echo x'` wouldn't even start in
        // most distro layouts. Regression guard for over-aggressive clearing.
        let out = execute_headless_prompt(
            "sh".to_string(),
            vec!["-c".to_string(), "echo \"${PATH:+PATH_OK}\"".to_string()],
            None,
            5000,
            "/tmp".to_string(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(out, "PATH_OK");
    }
}
