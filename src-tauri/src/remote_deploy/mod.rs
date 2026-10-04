pub(crate) mod assets;
pub(crate) mod service;

use std::path::Path;
use std::time::Duration;

use crate::tunnels::classifier::ExitReason;
use crate::tunnels::exec::{scp_push_with_binaries, ssh_exec_with_binary};
use crate::tunnels::profile::TunnelProfile;

const REMOTE_BINARY: &str = "~/.cache/tuic/tuic-remote";
const REMOTE_LOG: &str = "~/.cache/tuic/tuic-remote.log";
const REMOTE_PID: &str = "~/.cache/tuic/tuic-remote.pid";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeployError {
    Uname(ExitReason),
    Asset(String),
    Push(ExitReason),
    Launch(String),
}

impl std::fmt::Display for DeployError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uname(reason) => write!(formatter, "remote platform probe failed: {reason:?}"),
            Self::Asset(reason) => write!(formatter, "release asset failed: {reason}"),
            Self::Push(reason) => write!(formatter, "remote binary push failed: {reason:?}"),
            Self::Launch(reason) => write!(formatter, "remote daemon launch failed: {reason}"),
        }
    }
}

impl std::error::Error for DeployError {}

pub(crate) async fn deploy_ephemeral(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
) -> Result<(), DeployError> {
    deploy_ephemeral_inner(
        profile,
        port,
        token,
        survive_secs,
        Path::new("ssh"),
        Path::new("scp"),
    )
    .await
}

pub(crate) async fn stop_ephemeral(profile: &TunnelProfile) -> Result<(), ExitReason> {
    stop_ephemeral_with_binary(profile, Path::new("ssh")).await
}

#[cfg(test)]
async fn deploy_ephemeral_with_binaries(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
    ssh_binary: &Path,
    scp_binary: &Path,
) -> Result<(), DeployError> {
    deploy_ephemeral_inner(profile, port, token, survive_secs, ssh_binary, scp_binary).await
}

async fn deploy_ephemeral_inner(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
    ssh_binary: &Path,
    scp_binary: &Path,
) -> Result<(), DeployError> {
    if token.is_empty() || token.contains(['\r', '\n']) {
        return Err(DeployError::Launch("invalid pairing token".to_string()));
    }

    let uname = ssh_exec_with_binary(profile, "uname -sm", None, COMMAND_TIMEOUT, ssh_binary)
        .await
        .map_err(DeployError::Uname)?;
    let uname = uname.stdout.trim();
    let target = assets::require_target(uname).map_err(DeployError::Asset)?;
    let asset = assets::resolve_update_asset(target)
        .await
        .map_err(DeployError::Asset)?;
    let asset = asset.binary;

    let hash_command = format!(
        "mkdir -p ~/.cache/tuic && if [ -f {REMOTE_BINARY} ]; then (sha256sum {REMOTE_BINARY} 2>/dev/null || shasum -a 256 {REMOTE_BINARY} 2>/dev/null || true); fi"
    );
    let remote_hash =
        ssh_exec_with_binary(profile, &hash_command, None, COMMAND_TIMEOUT, ssh_binary)
            .await
            .ok()
            .and_then(|output| output.stdout.split_whitespace().next().map(str::to_owned));

    if remote_hash.as_deref() != Some(asset.sha256.as_str()) {
        scp_push_with_binaries(
            profile,
            &asset.path,
            REMOTE_BINARY,
            PUSH_TIMEOUT,
            scp_binary,
            ssh_binary,
        )
        .await
        .map_err(DeployError::Push)?;
    }

    let launch_command = format!(
        "read -r T; cd ~/.cache/tuic && if [ -f tuic-remote.pid ]; then kill $(cat tuic-remote.pid) 2>/dev/null || true; fi; if command -v setsid >/dev/null 2>&1; then TUIC_PAIRING_TOKEN=$T TUIC_PORT={port} setsid nohup ./tuic-remote --bind 127.0.0.1 --survive-secs {survive_secs} --no-agent-configs >tuic-remote.log 2>&1 </dev/null & else TUIC_PAIRING_TOKEN=$T TUIC_PORT={port} nohup ./tuic-remote --bind 127.0.0.1 --survive-secs {survive_secs} --no-agent-configs >tuic-remote.log 2>&1 </dev/null & fi; P=$!; echo $P > tuic-remote.pid; unset T; (while kill -0 \"$P\" 2>/dev/null; do sleep 1; done; if [ \"$(cat tuic-remote.pid 2>/dev/null)\" = \"$P\" ]; then rm -f tuic-remote.pid; fi) >/dev/null 2>&1 </dev/null & sleep 1; kill -0 \"$P\" 2>/dev/null"
    );
    let stdin = format!("{token}\n");
    if let Err(reason) = ssh_exec_with_binary(
        profile,
        &launch_command,
        Some(stdin.as_bytes()),
        LAUNCH_TIMEOUT,
        ssh_binary,
    )
    .await
    {
        let tail = ssh_exec_with_binary(
            profile,
            &format!("tail -n 5 {REMOTE_LOG} 2>/dev/null || true"),
            None,
            COMMAND_TIMEOUT,
            ssh_binary,
        )
        .await
        .map(|output| output.stdout)
        .unwrap_or_default();
        return Err(DeployError::Launch(format!(
            "{reason:?}; last remote log lines:\n{}",
            tail.trim_end()
        )));
    }

    Ok(())
}

async fn stop_ephemeral_with_binary(
    profile: &TunnelProfile,
    ssh_binary: &Path,
) -> Result<(), ExitReason> {
    let command = format!(
        "if [ -f {REMOTE_PID} ]; then kill $(cat {REMOTE_PID}) 2>/dev/null || true; rm -f {REMOTE_PID}; fi"
    );
    ssh_exec_with_binary(profile, &command, None, STOP_TIMEOUT, ssh_binary)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::test_support::{fail_with_stderr_script, fake_ssh_script, system32_exe};
    use crate::tunnels::classifier::ExitReason;
    use crate::tunnels::profile::TunnelProfile;

    fn profile() -> TunnelProfile {
        TunnelProfile::new("deploy", "example.com", "alice")
    }

    fn cached_asset(config: &Path, bytes: &[u8]) -> String {
        let target = "x86_64-unknown-linux-gnu";
        let path = config
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"))
            .join(format!("tuic-remote-{target}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    fn remove_log(binary: &Path) {
        let _ = std::fs::remove_file(format!("{}.log", binary.display()));
        let _ = std::fs::remove_file(format!("{}.stdin", binary.display()));
        let _ = std::fs::remove_file(format!("{}.args", binary.display()));
    }

    fn scripted_ssh(name: &str, remote_hash: &str, launch_ok: bool) -> PathBuf {
        let launch = if launch_ok {
            "IFS= read -r token; printf '%s' \"$token\" > \"$0.stdin\"; exit 0"
        } else {
            "exit 1"
        };
        let posix = format!(
            "printf '%s\\n' \"$*\" >> \"$0.log\"\ncase \"$*\" in\n  *\"uname -sm\"*) printf 'Linux x86_64\\n'; exit 0;;\n  *\"sha256sum\"*) printf '{remote_hash}  tuic-remote\\n'; exit 0;;\n  *\"read -r T\"*) {launch};;\n  *\"tail -n 5\"*) printf 'line1\\nline2\\nline3\\nline4\\nAddress already in use\\n'; exit 0;;\nesac\nexit 0"
        );
        let windows_launch = if launch_ok {
            "set /p token=\r\n<nul set /p \"=!token!\">\"%~f0.stdin\"\r\nexit /b 0"
        } else {
            "exit /b 1"
        };
        let findstr = system32_exe("findstr.exe");
        let windows = format!(
            "setlocal EnableDelayedExpansion\r\nset \"last=\"\r\n:args\r\nif \"%~1\"==\"\" goto args_done\r\nset \"last=%~1\"\r\nshift /1\r\ngoto args\r\n:args_done\r\necho(!last!>>\"%~f0.log\"\r\necho(!last!>\"%~f0.args\"\r\n{findstr} /C:\"uname -sm\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo Linux x86_64& exit /b 0)\r\n{findstr} /C:\"sha256sum\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo {remote_hash}  tuic-remote& exit /b 0)\r\n{findstr} /C:\"read -r T\" \"%~f0.args\" >nul\r\nif not errorlevel 1 ({windows_launch})\r\n{findstr} /C:\"tail -n 5\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo line1& echo line2& echo line3& echo line4& echo Address already in use& exit /b 0)\r\nexit /b 0"
        );
        fake_ssh_script(name, &posix, &windows)
    }

    #[tokio::test]
    async fn matching_remote_hash_skips_scp_and_token_stays_out_of_arguments() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let hash = cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_hash_match", &hash, true);
        let scp = fake_ssh_script(
            "deploy_scp_must_not_run",
            "printf called > \"$0.log\"; exit 0",
            "echo called > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);
        remove_log(&scp);

        deploy_ephemeral_with_binaries(&profile(), 9877, "pair-secret", 1_800, &ssh, &scp)
            .await
            .expect("deploy succeeds");

        assert!(!Path::new(&format!("{}.log", scp.display())).exists());
        let ssh_log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        // Rust's batch argument encoding doubles internal quotes. Decode that
        // fixture representation before checking the remote POSIX command.
        #[cfg(windows)]
        let ssh_log = ssh_log.replace("\"\"", "\"");
        assert!(!ssh_log.contains("pair-secret"));
        assert_eq!(
            std::fs::read_to_string(format!("{}.stdin", ssh.display())).unwrap(),
            "pair-secret"
        );
        assert!(ssh_log.contains("--survive-secs 1800"));
        assert!(ssh_log.contains("TUIC_PORT=9877"));
        assert!(ssh_log.contains("[ \"$(cat tuic-remote.pid 2>/dev/null)\" = \"$P\" ]"));
        assert!(ssh_log.contains("rm -f tuic-remote.pid"));
    }

    #[tokio::test]
    async fn mismatched_remote_hash_pushes_before_launch() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_hash_mismatch", "deadbeef", true);
        let scp = fake_ssh_script(
            "deploy_scp_runs",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);
        remove_log(&scp);

        deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &scp)
            .await
            .expect("deploy succeeds");

        let scp_log = std::fs::read_to_string(format!("{}.log", scp.display())).unwrap();
        assert!(scp_log.contains("tuic-remote.tmp-"));
        let ssh_log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        assert!(ssh_log.contains("$HOME/"));
        assert!(!ssh_log.contains("mv -f '~/.cache"));
    }

    #[tokio::test]
    async fn failing_steps_keep_their_deploy_error_variant() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        cached_asset(config.path(), b"binary");

        let failure = fail_with_stderr_script("Permission denied", 255);
        let uname_ssh = fake_ssh_script("deploy_uname_fails", &failure, &failure);
        let error =
            deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &uname_ssh, &uname_ssh)
                .await
                .unwrap_err();
        assert!(matches!(error, DeployError::Uname(ExitReason::AuthFailed)));

        let unsupported = fake_ssh_script(
            "deploy_asset_fails",
            "printf 'Plan9 mips\\n'; exit 0",
            "echo Plan9 mips\r\nexit /b 0",
        );
        let error = deploy_ephemeral_with_binaries(
            &profile(),
            9877,
            "token",
            60,
            &unsupported,
            &unsupported,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, DeployError::Asset(message) if message.contains("Plan9 mips")));

        let ssh = scripted_ssh("deploy_before_push_failure", "deadbeef", true);
        let push_failure = fail_with_stderr_script("Connection refused", 255);
        let scp = fake_ssh_script("deploy_push_fails", &push_failure, &push_failure);
        let error = deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &scp)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DeployError::Push(ExitReason::ConnectionRefused)
        ));
    }

    #[tokio::test]
    async fn launch_failure_includes_the_last_five_log_lines() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let hash = cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_launch_fails", &hash, false);

        let error = deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &ssh)
            .await
            .unwrap_err();

        assert!(
            matches!(error, DeployError::Launch(message) if message.contains("Address already in use") && message.contains("line1"))
        );
    }

    #[tokio::test]
    async fn stop_ephemeral_uses_the_pid_file_and_is_bounded() {
        let ssh = fake_ssh_script(
            "deploy_stop",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);

        stop_ephemeral_with_binary(&profile(), &ssh)
            .await
            .expect("best-effort stop command runs");

        let args = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        assert!(args.contains("tuic-remote.pid"));
        assert!(args.contains("kill"));
        assert_eq!(STOP_TIMEOUT, Duration::from_secs(10));
    }
}
