//! `op` adapter. Only `op read` is used; stdin is closed so it never waits on a
//! terminal. A sign-in or biometric prompt the user does not answer ends in
//! `Timeout` or `Locked`, never in a stored value.
use super::provider::{ProviderError, SecretProvider, SecretRef};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub(crate) struct OpOutput {
    pub success: bool,
    pub stdout: Zeroizing<String>,
    pub stderr: String,
}

/// Runs the CLI. The real one spawns a process; tests use the fake script.
pub(crate) trait OpRunner {
    fn read(&self, reference: &str, account: Option<&str>) -> Result<OpOutput, ProviderError>;
}

pub(crate) struct CommandOpRunner {
    program: String,
    prefix_args: Vec<String>,
    timeout: Duration,
}

impl CommandOpRunner {
    pub(crate) fn new(program: &str, prefix_args: &[&str], timeout: Duration) -> Self {
        Self {
            program: program.to_owned(),
            prefix_args: prefix_args.iter().map(|a| (*a).to_owned()).collect(),
            timeout,
        }
    }
}

impl OpRunner for CommandOpRunner {
    fn read(&self, reference: &str, account: Option<&str>) -> Result<OpOutput, ProviderError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.prefix_args)
            .args(["read", "--no-newline"]);
        if let Some(account) = account {
            command.args(["--account", account]);
        }
        let mut child = command
            .arg(reference)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ProviderError::OpMissing,
                _ => ProviderError::Failed,
            })?;
        let deadline = Instant::now() + self.timeout;
        // Pipes are drained from spawn on, under the same deadline: a helper that
        // outlives `op` and keeps stdout open must not hold the call past it.
        let drain = |pipe: Option<Box<dyn Read + Send>>| {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let mut text = Zeroizing::new(String::new());
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_string(&mut text);
                }
                let _ = tx.send(text);
            });
            rx
        };
        let out_rx = drain(child.stdout.take().map(|p| Box::new(p) as _));
        let err_rx = drain(child.stderr.take().map(|p| Box::new(p) as _));
        let expired = |mut child: std::process::Child| {
            let _ = child.kill();
            let _ = child.wait();
            ProviderError::Timeout
        };
        let left = |deadline: Instant| deadline.saturating_duration_since(Instant::now());
        let Ok(stdout) = out_rx.recv_timeout(left(deadline)) else {
            return Err(expired(child));
        };
        let Ok(stderr) = err_rx.recv_timeout(left(deadline)) else {
            return Err(expired(child));
        };
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(None) => return Err(expired(child)),
                Err(_) => return Err(ProviderError::Failed),
            }
        };
        let stderr = String::from_utf8_lossy(stderr.as_bytes()).into_owned();
        Ok(OpOutput {
            success: status.success(),
            stdout,
            stderr,
        })
    }
}

/// Maps `op` stderr wording to a typed error. The wording is NOT verified
/// against a real CLI (no test may call it); unknown text maps to `Failed`.
fn classify(stderr: &str) -> ProviderError {
    let text = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    if has(&[
        "not currently signed in",
        "no accounts configured",
        "not signed in",
    ]) {
        ProviderError::NotSignedIn
    } else if has(&[
        "authorization prompt",
        "authorization timeout",
        "locked",
        "biometric",
    ]) {
        ProviderError::Locked
    } else if has(&[
        "isn't an item",
        "isn't a vault",
        "isn't a field",
        "could not be found",
        "not found",
    ]) {
        ProviderError::UnknownReference
    } else {
        ProviderError::Failed
    }
}

fn valid_account(account: &str) -> bool {
    !account.is_empty()
        && !account.starts_with('-')
        && account
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))
}

pub(crate) struct OpProvider<R: OpRunner>(pub R);

impl<R: OpRunner> SecretProvider for OpProvider<R> {
    fn resolve(&self, reference: &SecretRef) -> Result<Zeroizing<String>, ProviderError> {
        let SecretRef::Op { reference, account } = reference;
        if !reference.starts_with("op://")
            || reference.chars().any(|c| c.is_control())
            || account.as_deref().is_some_and(|a| !valid_account(a))
        {
            return Err(ProviderError::Invalid);
        }
        let output = self.0.read(reference, account.as_deref())?;
        if !output.success {
            return Err(classify(&output.stderr));
        }
        if !super::valid_secret_value(&output.stdout) {
            return Err(ProviderError::Invalid);
        }
        Ok(output.stdout)
    }
}
