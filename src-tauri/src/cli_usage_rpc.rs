//! Small JSONL JSON-RPC client for provider-owned usage surfaces.
//!
//! Codex App Server and Grok ACP both own authentication and upstream schema
//! translation. TUICommander launches them only long enough to request an
//! account snapshot; OAuth tokens never cross this process boundary.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Shared five-minute freshness and thirty-minute stale fallback for CLI usage.
pub(crate) struct UsageCache<T>(parking_lot::Mutex<Option<(T, Instant)>>);

impl<T: Clone> UsageCache<T> {
    pub(crate) const fn new() -> Self {
        Self(parking_lot::Mutex::new(None))
    }

    pub(crate) fn fresh(&self) -> Option<T> {
        self.get(Duration::from_secs(300))
    }

    pub(crate) fn stale(&self) -> Option<T> {
        self.get(Duration::from_secs(1800))
    }

    fn get(&self, max_age: Duration) -> Option<T> {
        let guard = self.0.lock();
        let (value, at) = guard.as_ref()?;
        (at.elapsed() < max_age).then(|| value.clone())
    }

    pub(crate) fn put(&self, value: &T) {
        *self.0.lock() = Some((value.clone(), Instant::now()));
    }
}

/// Failure cooldown for a provider usage reader: after a failed read, callers inside
/// the window get the cached error instead of spawning the CLI again. The delay doubles on
/// repeats (60 s up to 600 s) and follows a retry delay the CLI reports (clamped to 1800 s).
pub(crate) struct FailureBackoff(parking_lot::Mutex<Option<(String, Instant, u32)>>);

impl Default for FailureBackoff {
    fn default() -> Self {
        Self::new()
    }
}

impl FailureBackoff {
    pub(crate) const fn new() -> Self {
        Self(parking_lot::Mutex::new(None))
    }

    /// The error of the last failed read while its cooldown runs.
    pub(crate) fn active(&self) -> Option<String> {
        let guard = self.0.lock();
        let (error, until, _) = guard.as_ref()?;
        (Instant::now() < *until).then(|| error.clone())
    }

    pub(crate) fn record(&self, error: &str) {
        let mut guard = self.0.lock();
        let failures = guard.as_ref().map_or(0, |(_, _, n)| *n).saturating_add(1);
        let delay = reported_retry_secs(error).map_or_else(
            || (60u64 << (failures - 1).min(4)).min(600),
            |secs| secs.clamp(1, 1800),
        );
        *guard = Some((
            error.to_string(),
            Instant::now() + Duration::from_secs(delay),
            failures,
        ));
    }

    pub(crate) fn clear(&self) {
        *self.0.lock() = None;
    }

    #[cfg(test)]
    pub(crate) fn remaining(&self) -> Option<Duration> {
        let guard = self.0.lock();
        let (_, until, _) = guard.as_ref()?;
        Some(until.saturating_duration_since(Instant::now()))
    }
}

/// Seconds a CLI error asks us to wait ("retry after 90", "try again in 90s", "retry-after: 90").
fn reported_retry_secs(message: &str) -> Option<u64> {
    let lower = message.to_ascii_lowercase();
    ["retry after ", "retry-after: ", "try again in "]
        .iter()
        .find_map(|marker| {
            let rest = &lower[lower.find(marker)? + marker.len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
}

pub(crate) fn looks_like_auth_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not logged in")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("401")
        || lower.contains("403")
}

pub(crate) fn looks_like_contract_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    message.contains("Failed to parse")
        || message.contains("returned no result")
        || message.contains("returned no response")
        || lower.contains("method not found")
        || lower.contains("unknown method")
        || lower.contains("unsupported method")
        || lower.contains("configuration invalid; defaults rejected")
}

fn record_response(line: &str, wanted: &HashSet<u64>, responses: &mut HashMap<u64, Value>) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let Some(id) = value.get("id").and_then(Value::as_u64) else {
        return;
    };
    if wanted.contains(&id) {
        responses.insert(id, value);
    }
}

/// Run a provider CLI over its newline-delimited JSON-RPC stdio transport.
///
/// The child is stopped as soon as every requested response arrives. Keeping
/// stdin open until then matters: both servers treat EOF as host shutdown and
/// may abandon an in-flight network request.
pub async fn request_jsonl(
    program: &Path,
    args: &[&str],
    messages: &[Value],
    response_ids: &[u64],
    timeout: Duration,
) -> Result<HashMap<u64, Value>, String> {
    request_jsonl_with_env(program, args, messages, response_ids, timeout, &[]).await
}

/// Same protocol, bound to a live session's home/profile rather than this host's defaults.
pub(crate) async fn request_jsonl_with_env(
    program: &Path,
    args: &[&str],
    messages: &[Value],
    response_ids: &[u64],
    timeout: Duration,
    environment: &[(String, String)],
) -> Result<HashMap<u64, Value>, String> {
    let mut command = std::process::Command::new(program);
    command.envs(environment.iter().cloned());
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Provider logs are verbose and can contain account metadata. Protocol
        // errors are returned on stdout, so stderr is deliberately discarded.
        .stderr(Stdio::null());
    crate::cli::apply_no_window(&mut command);
    // `kill_on_drop` belongs to tokio's Command, not std's, so it is set after
    // the conversion — the builder above is std because `apply_no_window` is.
    let mut command: tokio::process::Command = command.into();
    command.kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start {}: {error}", program.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Provider usage process has no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Provider usage process has no stdout".to_string())?;

    for message in messages {
        let mut encoded = serde_json::to_vec(message)
            .map_err(|error| format!("Failed to encode provider usage request: {error}"))?;
        encoded.push(b'\n');
        stdin
            .write_all(&encoded)
            .await
            .map_err(|error| format!("Failed to write provider usage request: {error}"))?;
    }
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush provider usage request: {error}"))?;

    let wanted: HashSet<u64> = response_ids.iter().copied().collect();
    let read = async {
        let mut lines = BufReader::new(stdout).lines();
        let mut responses = HashMap::new();
        while responses.len() < wanted.len() {
            let line = lines
                .next_line()
                .await
                .map_err(|error| format!("Failed to read provider usage response: {error}"))?
                .ok_or_else(|| "Provider usage process exited before responding".to_string())?;
            // Codex can keep serving account RPCs after discarding an invalid
            // configuration. Never publish that default account as selected usage.
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && value.get("method").and_then(Value::as_str) == Some("configWarning")
                && value.pointer("/params/summary").and_then(Value::as_str)
                    == Some("Invalid configuration; using defaults.")
            {
                return Err("Provider usage configuration invalid; defaults rejected".to_string());
            }
            record_response(&line, &wanted, &mut responses);
        }
        Ok::<_, String>(responses)
    };

    let result = tokio::time::timeout(timeout, read).await.map_err(|_| {
        format!(
            "Provider usage request timed out after {}s",
            timeout.as_secs()
        )
    })?;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

/// Extract and deserialize one successful JSON-RPC result.
pub fn decode_result<T: serde::de::DeserializeOwned>(
    responses: &HashMap<u64, Value>,
    id: u64,
    label: &str,
) -> Result<T, String> {
    let response = responses
        .get(&id)
        .ok_or_else(|| format!("{label} returned no response"))?;
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown JSON-RPC error");
        return Err(format!("{label} failed: {message}"));
    }
    let result = response
        .get("result")
        .ok_or_else(|| format!("{label} returned no result"))?;
    serde_json::from_value(result.clone())
        .map_err(|error| format!("Failed to parse {label} response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: a backoff that never grows, so a persistent 429 is retried every minute.
    #[test]
    fn failure_backoff_grows_on_repeats_is_capped_and_honours_a_reported_delay() {
        let backoff = FailureBackoff::new();
        assert!(backoff.active().is_none());
        let mut seen = Vec::new();
        for _ in 0..7 {
            backoff.record("429 Too Many Requests");
            seen.push(backoff.remaining().unwrap().as_secs().div_ceil(10) * 10);
        }
        assert_eq!(seen, [60, 120, 240, 480, 600, 600, 600]);
        backoff.record("429 retry after 900s");
        assert!(backoff.remaining().unwrap().as_secs() > 600);
        backoff.clear();
        assert!(backoff.active().is_none());
    }

    #[test]
    fn records_only_requested_numeric_responses() {
        let wanted = HashSet::from([2]);
        let mut responses = HashMap::new();
        record_response(
            r#"{"method":"notice","params":{}}"#,
            &wanted,
            &mut responses,
        );
        record_response(r#"{"id":"2","result":{}}"#, &wanted, &mut responses);
        record_response(r#"{"id":1,"result":{}}"#, &wanted, &mut responses);
        record_response(r#"{"id":2,"result":{"ok":true}}"#, &wanted, &mut responses);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[&2]["result"]["ok"], true);
    }

    #[test]
    fn surfaces_json_rpc_errors_without_exposing_the_payload() {
        let responses = HashMap::from([(
            7,
            serde_json::json!({"id": 7, "error": {"code": -1, "message": "not logged in"}}),
        )]);
        let error = decode_result::<Value>(&responses, 7, "Grok billing").unwrap_err();
        assert_eq!(error, "Grok billing failed: not logged in");
    }
}
