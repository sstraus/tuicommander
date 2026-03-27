use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Execute a headless (one-shot) agent command and capture stdout.
///
/// `command_line` is the full shell command (e.g. `claude -p "review this code"`).
/// `stdin_content` is piped to the process stdin to avoid shell injection — prompt
/// content must NEVER be interpolated into the command_line string.
/// Uses platform-appropriate shell for argument parsing. Timeout is capped at 5 minutes.
#[tauri::command]
pub(crate) async fn execute_headless_prompt(
    command_line: String,
    stdin_content: Option<String>,
    timeout_ms: u64,
    repo_path: String,
) -> Result<String, String> {
    let duration = Duration::from_millis(timeout_ms.min(300_000)); // Cap at 5 minutes

    let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
    let shell_flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

    let needs_stdin = stdin_content.is_some();
    let mut child = Command::new(shell)
        .arg(shell_flag)
        .arg(&command_line)
        .current_dir(&repo_path)
        .stdin(if needs_stdin { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    // Pipe prompt content via stdin to avoid shell injection
    if let Some(content) = stdin_content {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(content.as_bytes()).await;
            drop(stdin); // Close stdin so the process can proceed
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn headless_echo_command() {
        let result =
            execute_headless_prompt("echo hello".to_string(), None, 5000, "/tmp".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn headless_stdin_piped() {
        let result = execute_headless_prompt(
            "cat".to_string(),
            Some("hello from stdin".to_string()),
            5000,
            "/tmp".to_string(),
        ).await;
        assert_eq!(result.unwrap(), "hello from stdin");
    }

    #[tokio::test]
    async fn headless_nonzero_exit() {
        let result =
            execute_headless_prompt("false".to_string(), None, 5000, "/tmp".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn headless_timeout() {
        let result =
            execute_headless_prompt("sleep 10".to_string(), None, 100, "/tmp".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Timed out"));
    }
}
