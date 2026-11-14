//! Quota of ego-owned subscriptions. ego publishes one `rate_limits` contract
//! (usage report v6, `ego auth usage --json`); TUIC never reads ego's
//! credentials or a provider endpoint itself, and needs no other harness CLI.
use super::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

/// `ego auth usage` buffers at most 256 KiB of provider body; its output is smaller.
const MAX_OUTPUT: u64 = 256 * 1024;
/// ego bounds its own provider read to 15 s.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub(super) struct Ego {
    /// Resolved once; resolution probes the binary and is not cheap.
    program: parking_lot::Mutex<Option<PathBuf>>,
}

impl Ego {
    #[cfg(test)]
    pub(super) fn with_program(program: PathBuf) -> Self {
        Self {
            program: parking_lot::Mutex::new(Some(program)),
        }
    }

    async fn program(&self) -> Result<PathBuf, QuotaError> {
        if let Some(path) = self.program.lock().clone().filter(|p| p.is_file()) {
            return Ok(path);
        }
        let path = crate::agent::detect_agent_binary("ego".into())
            .await
            .path
            .map(PathBuf::from)
            .ok_or_else(|| QuotaError::Unavailable("ego executable unavailable".into()))?;
        *self.program.lock() = Some(path.clone());
        Ok(path)
    }
}

impl QuotaOwner for Ego {
    fn owner(&self) -> &'static str {
        "ego"
    }

    /// Metadata only: the store holds tokens, and any change may end an auth block.
    fn credential_fingerprint(&self, key: &SubscriptionKey) -> Option<Fingerprint> {
        let meta = std::fs::metadata(Path::new(&key.credential).join("auth.json")).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        Some(format!("{}:{}", meta.len(), modified.as_nanos()))
    }

    fn read<'a>(
        &'a self,
        key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
        Box::pin(async move {
            let program = self.program().await?;
            let output = auth_usage(&program, Path::new(&key.credential), &key.provider).await?;
            parse_auth_usage(&output, &key.provider, SystemClock.now_ms())
        })
    }
}

/// `<ego> auth usage <provider> --json` against the key's `EGO_HOME`. ego
/// answers from its own fresh snapshot when it has one and owns the provider
/// cadence across processes; TUIC only bounds the process.
async fn auth_usage(program: &Path, home: &Path, provider: &str) -> Result<Value, QuotaError> {
    let unavailable = || QuotaError::Unavailable("ego auth usage unavailable".into());
    let mut command = std::process::Command::new(program);
    command
        .args(["auth", "usage", provider, "--json"])
        .env("EGO_HOME", home)
        .env_remove("EGO_PROFILE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::cli::apply_no_window(&mut command);
    let mut command: tokio::process::Command = command.into();
    command.kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| unavailable())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(unavailable)?
        .take(MAX_OUTPUT + 1);
    let mut bytes = Vec::new();
    let read = async {
        stdout
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| unavailable())?;
        if bytes.len() as u64 > MAX_OUTPUT {
            return Err(QuotaError::Contract(
                "ego auth usage output too large".into(),
            ));
        }
        // An ego release without the command exits non-zero.
        if !child.wait().await.map_err(|_| unavailable())?.success() {
            return Err(unavailable());
        }
        Ok(())
    };
    let result = tokio::time::timeout(READ_TIMEOUT, read).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result.map_err(|_| QuotaError::Network("ego auth usage timed out".into()))??;
    serde_json::from_slice(&bytes)
        .map_err(|_| QuotaError::Contract("Failed to parse ego auth usage".into()))
}

/// The provider's issue wins over a quota printed with it: ego then returns
/// its prior value, and the registry already keeps its own last good one.
fn parse_auth_usage(
    output: &Value,
    provider: &str,
    now_ms: u64,
) -> Result<QuotaSnapshot, QuotaError> {
    let issue = output["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| matches!(e["provider"].as_str(), Some(p) if p == provider || p == "all"));
    if let Some(issue) = issue {
        let message = issue["message"]
            .as_str()
            .unwrap_or("ego quota issue")
            .to_owned();
        let retry_at = issue["retry_at"].as_u64();
        return Err(match issue["state"].as_str() {
            Some("auth_invalid") => QuotaError::Auth(message),
            Some("backoff_until") => QuotaError::RateLimited {
                retry_after_secs: retry_at.map(|at| at.saturating_sub(now_ms) / 1000),
            },
            // A retry time means a transient failure; none means no source at all.
            Some("unavailable") if retry_at.is_some() => QuotaError::Network(message),
            Some("unavailable") => QuotaError::Unavailable(message),
            _ => QuotaError::Contract("Unknown ego quota issue".into()),
        });
    }
    rate_limits(&output["rate_limits"])
        .into_iter()
        .find(|(p, _)| p == provider)
        .map(|(_, snapshot)| snapshot)
        .ok_or_else(|| QuotaError::Contract("No ego quota for provider".into()))
}

/// Model scope of an ego window name, spelled as the native adapters spell it:
/// `seven_day_opus` -> `opus`, `code_review_*` -> `code review`. `seven_day`,
/// `five_hour` and their `_primary`/`_secondary` forms are account-wide.
fn window_model(name: &str) -> Option<String> {
    if name.starts_with("code_review") {
        return Some("code review".into());
    }
    let scope = ["seven_day_", "five_hour_"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))?;
    (!matches!(scope, "primary" | "secondary")).then(|| scope.replace('_', " "))
}

/// One snapshot per provider from a `rate_limits` array. ego keeps only the
/// selected credential of each provider.
fn rate_limits(value: &Value) -> Vec<(String, QuotaSnapshot)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(rate_limit)
        .collect()
}

fn rate_limit(entry: &Value) -> Option<(String, QuotaSnapshot)> {
    let (Some(provider), Some(observed_at_ms)) =
        (entry["provider"].as_str(), entry["observed_at"].as_u64())
    else {
        return None;
    };
    let windows: Vec<_> = entry["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| {
            QuotaWindow::from_duration(
                w["used_percentage"].as_f64()?,
                w["window_seconds"].as_i64(),
                w["resets_at"]
                    .as_i64()
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .map(|d| d.to_rfc3339()),
                w["name"].as_str().and_then(window_model),
            )
        })
        .collect();
    if windows.is_empty() {
        return None;
    }
    let snapshot = QuotaSnapshot {
        windows,
        detail: None,
        plan: entry["plan"].as_str().map(str::to_owned),
        credits: entry["credits"]["balance"].as_str().map(str::to_owned),
        account: None,
        observed_at_ms,
        source: if entry["source"] == "passive" {
            QuotaSource::Passive
        } else {
            QuotaSource::Active
        },
    };
    Some((provider.to_owned(), snapshot))
}

/// Passive source: the `rate_limits` of a usage report v6 that the session
/// usage reader already acquired for this `EGO_HOME`. No extra process.
pub(crate) fn observe_usage_report(registry: &QuotaRegistry, home: &Path, report: &Value) {
    let home = home.to_string_lossy();
    for (provider, snapshot) in rate_limits(&report["rate_limits"]) {
        registry.observe(
            &SubscriptionKey::new("ego", provider, home.as_ref()),
            snapshot,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> Value {
        serde_json::from_str(include_str!("fixtures/ego-contract.json")).unwrap()
    }

    // Catches: ego quota silently depends on another harness (a provider CLI on
    // PATH or another owner's credential). The child gets a real empty PATH.
    #[cfg(unix)]
    #[test]
    fn ego_key_reads_quota_with_no_provider_cli_installed() {
        const CHILD: &str = "TUIC_EGO_QUOTA_NO_CLI_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "provider_quota::ego::tests::ego_key_reads_quota_with_no_provider_cli_installed",
                    "--nocapture",
                ])
                .env("PATH", dir.path())
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        for cli in ["claude", "codex", "grok"] {
            let error = std::process::Command::new(cli).output().unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{cli}");
        }
        // Absolute paths only: the child has no PATH.
        let script = crate::test_support::fake_ssh_script(
            "2993-ego-auth-usage",
            r#"[ "$1 $2 $3 $4" = "auth usage anthropic --json" ] || exit 2
exec /bin/cat "$EGO_HOME/auth-usage.json""#,
            "exit /b 2",
        );
        let home = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let output = serde_json::json!({"rate_limits": contract()["rate_limits"], "errors": []});
        std::fs::write(home.path().join("auth-usage.json"), output.to_string()).unwrap();
        let key = SubscriptionKey::new("ego", "anthropic", home.path().to_string_lossy());
        let registry = QuotaRegistry::default();
        super::super::adapters::register(&registry);
        // Replaces the registered ego owner only to pin the executable.
        registry.register_owner(Arc::new(Ego::with_program(script)));
        let view = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(registry.ensure(&key));
        let snapshot = view.snapshot.expect("ego quota without a provider CLI");
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].used_percent, 82.0);
        assert_eq!(snapshot.windows[0].kind, WindowKind::Weekly);
    }

    // Catches: an issue mapped to the wrong registry class, e.g. an expired
    // login retried as a network failure, or a missing endpoint backing off forever.
    #[test]
    fn ego_auth_usage_issues_map_to_registry_failure_classes() {
        let issue = |state: &str, retry_at: Option<u64>| {
            serde_json::json!({
                "rate_limits": contract()["rate_limits"],
                "errors": [{"provider": "anthropic", "state": state, "message": "m", "retry_at": retry_at}]
            })
        };
        let read = |v: Value| parse_auth_usage(&v, "anthropic", 1_000_000);
        assert_eq!(
            read(issue("auth_invalid", None)),
            Err(QuotaError::Auth("m".into()))
        );
        assert_eq!(
            read(issue("backoff_until", Some(1_090_000))),
            Err(QuotaError::RateLimited {
                retry_after_secs: Some(90)
            })
        );
        assert_eq!(
            read(issue("unavailable", Some(1_300_000))),
            Err(QuotaError::Network("m".into()))
        );
        assert_eq!(
            read(issue("unavailable", None)),
            Err(QuotaError::Unavailable("m".into()))
        );
        // An issue of another provider does not hide this provider's quota.
        let mut other = issue("auth_invalid", None);
        other["errors"][0]["provider"] = "openai-codex".into();
        assert_eq!(read(other).unwrap().windows[0].used_percent, 82.0);
    }

    // Catches: a model-scoped ego window (`seven_day_opus`) reported as the
    // account-wide weekly window, so the user reads the Opus limit as the account limit.
    #[test]
    fn ego_model_scoped_window_is_not_reported_as_account_wide() {
        let output = serde_json::json!({
            "rate_limits": [{
                "provider": "anthropic",
                "credential": "oauth:example",
                "windows": [
                    {"name": "seven_day", "used_percentage": 40.0, "resets_at": 1794502800, "window_seconds": 604800},
                    {"name": "seven_day_opus", "used_percentage": 90.0, "resets_at": 1794502800, "window_seconds": 604800}
                ],
                "plan": null,
                "credits": null,
                "observed_at": 1794458253279u64,
                "source": "active"
            }],
            "errors": []
        });
        let snapshot = parse_auth_usage(&output, "anthropic", 1_794_458_260_000).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        let account_wide: Vec<f64> = snapshot
            .windows
            .iter()
            .filter(|w| w.model.is_none())
            .map(|w| w.used_percent)
            .collect();
        assert_eq!(account_wide, vec![40.0]);
    }

    // Catches: one provider's windows observed under another provider's key.
    #[test]
    fn usage_report_v6_observes_each_provider_under_its_own_key() {
        let mut report: Value =
            serde_json::from_str(include_str!("../fixtures/ego-usage-v5.json")).unwrap();
        report["v"] = 6.into();
        report["rate_limits"] = contract()["rate_limits"].clone();
        assert!(crate::ego_cli::usage::validate_report(&report).is_ok());
        let registry = QuotaRegistry::default();
        let home = Path::new("/ego-home");
        observe_usage_report(&registry, home, &report);
        let codex = registry.get(&SubscriptionKey::new("ego", "openai-codex", "/ego-home"));
        let codex = codex.snapshot.expect("codex observed");
        assert_eq!(codex.windows[0].used_percent, 14.0);
        assert_eq!(codex.plan.as_deref(), Some("pro"));
        let claude = registry.get(&SubscriptionKey::new("ego", "anthropic", "/ego-home"));
        assert_eq!(
            claude.snapshot.expect("anthropic observed").windows[0].used_percent,
            82.0
        );
    }
}
