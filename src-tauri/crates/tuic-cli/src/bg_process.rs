//! A detached job has a finite lifetime and owns exactly one child group.
use std::{
    process::{Child, Command},
    time::{Duration, Instant},
};

pub(crate) fn timeout() -> Result<Duration, String> {
    let seconds = match std::env::var("TUIC_BG_TIMEOUT_SECONDS") {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| "Invalid TUIC_BG_TIMEOUT_SECONDS".to_string())?,
        Err(std::env::VarError::NotPresent) => 3600,
        Err(error) => return Err(error.to_string()),
    };
    if !(1..=86400).contains(&seconds) {
        return Err("TUIC_BG_TIMEOUT_SECONDS must be between 1 and 86400".into());
    }
    Ok(Duration::from_secs(seconds))
}

pub(crate) fn run(command: &mut Command, timeout: Duration) -> Result<(i32, bool), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok((status.code().unwrap_or(1), false));
        }
        if started.elapsed() >= timeout {
            stop(&mut child)?;
            child.wait().map_err(|e| e.to_string())?;
            return Ok((124, true));
        }
        std::thread::sleep(
            Duration::from_millis(100).min(timeout.saturating_sub(started.elapsed())),
        );
    }
}

#[cfg(unix)]
fn stop(child: &mut Child) -> Result<(), String> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let pid = i32::try_from(child.id()).map_err(|e| e.to_string())?;
    // SAFETY: this process created this child's group with process_group(0).
    // The unreaped Child holds its PID, so the group cannot be a recycled job.
    if unsafe { kill(-pid, 9) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    // An already-exited child can lose its group between try_wait and kill.
    if error.raw_os_error() == Some(3) {
        return Ok(());
    }
    Err(format!(
        "Cannot stop owned background process group {pid}: {error}"
    ))
}

#[cfg(windows)]
fn stop(child: &mut Child) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() || child.try_wait().map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }
    Err(format!(
        "Cannot stop owned background process tree {}",
        child.id()
    ))
}
