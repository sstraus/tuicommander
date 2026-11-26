//! Quota of Grok CLI subscriptions. The active read is the Grok CLI's own ACP
//! billing request (`grok agent stdio`, `_x.ai/billing`) against the key's
//! `GROK_HOME`: the CLI keeps its login, and TUIC never reads the token store
//! (decision D6, Grok part, 2026-11-14). Grok records no quota a passive reader
//! could observe.
use super::*;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct GrokCli {
    /// Resolved once; resolution probes the binary and is not cheap.
    program: parking_lot::Mutex<Option<PathBuf>>,
}

impl GrokCli {
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
        let path = crate::agent::detect_agent_binary("grok".into())
            .await
            .path
            .map(PathBuf::from)
            .ok_or_else(|| QuotaError::Unavailable("Grok CLI not found".into()))?;
        *self.program.lock() = Some(path.clone());
        Ok(path)
    }
}

fn login_file(key: &SubscriptionKey) -> PathBuf {
    Path::new(&key.credential).join("auth.json")
}

impl QuotaOwner for GrokCli {
    fn owner(&self) -> &'static str {
        "grok-cli"
    }

    fn unavailable_reason(&self, key: &SubscriptionKey) -> Option<&'static str> {
        if key.profile.is_some() {
            Some("profile not supported")
        } else if !login_file(key).is_file() {
            Some("login not found")
        } else {
            None
        }
    }

    /// Metadata only: the file holds tokens. `grok login` and the CLI's token
    /// refresh rewrite it, which ends an auth block.
    fn credential_fingerprint(&self, key: &SubscriptionKey) -> Option<Fingerprint> {
        let meta = std::fs::metadata(login_file(key)).ok()?;
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
            if key.profile.is_some() {
                return Err(QuotaError::Unavailable("profile not supported".into()));
            }
            let program = self.program().await?;
            let response = crate::grok_usage::billing(&program, Path::new(&key.credential))
                .await
                .map_err(|message| classify(&message))?;
            grok_snapshot(response, SystemClock.now_ms())
        })
    }
}

/// Registry class of a failed CLI billing request.
fn classify(message: &str) -> QuotaError {
    use crate::cli_usage_rpc::{looks_like_auth_failure, looks_like_contract_failure};
    // Grok answers an expired login with "Invalid or expired credentials".
    if looks_like_auth_failure(message) || message.to_ascii_lowercase().contains("credentials") {
        QuotaError::Auth(message.to_owned())
    } else if looks_like_contract_failure(message) {
        QuotaError::Contract(message.to_owned())
    } else if message.starts_with("Failed to start") {
        QuotaError::Unavailable("Grok CLI not startable".into())
    } else {
        QuotaError::Network(message.to_owned())
    }
}

fn rfc3339(value: Option<&str>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value?).ok()
}

/// One window from `creditUsagePercent`; the current period supplies its
/// length and reset. A period date that is not RFC 3339 leaves the window
/// unclassified rather than guessed.
pub(super) fn grok_snapshot(
    response: crate::grok_usage::GrokUsageApiResponse,
    observed_at_ms: u64,
) -> Result<QuotaSnapshot, QuotaError> {
    let period = response.config.current_period.as_ref();
    let start = rfc3339(period.and_then(|p| p.start.as_deref()));
    let end = rfc3339(period.and_then(|p| p.end.as_deref()));
    let secs = start.zip(end).map(|(s, e)| (e - s).num_seconds());
    let window = response
        .config
        .credit_usage_percent
        .map(|pct| {
            QuotaWindow::from_duration(pct, secs, end.map(|e| e.to_rfc3339()), None)
                .ok_or_else(|| QuotaError::Contract("Invalid Grok quota percentage".into()))
        })
        .transpose()?;
    Ok(QuotaSnapshot {
        windows: window.into_iter().collect(),
        plan: response.subscription_tier.clone(),
        credits: None,
        account: None,
        observed_at_ms,
        source: QuotaSource::Active,
        detail: Some(QuotaDetail::Grok {
            observed_at_ms,
            response,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> crate::grok_usage::GrokUsageApiResponse {
        serde_json::from_str(include_str!("fixtures/grok-billing.json")).unwrap()
    }

    // Catches: a wrong window or plan after a renamed billing field.
    #[test]
    fn grok_billing_fixture_maps_to_windows() {
        let value = grok_snapshot(fixture(), 1).unwrap();
        assert_eq!(value.windows.len(), 1, "{value:?}");
        assert_eq!(value.windows[0].used_percent, 26.0);
        assert_eq!(value.windows[0].kind, WindowKind::Weekly);
        assert_eq!(
            value.windows[0].resets_at.as_deref(),
            Some("2026-11-16T00:00:00+00:00")
        );
        assert_eq!(value.plan.as_deref(), Some("SuperGrok"));
        // Catches: a period date in another format invents a window length.
        let mut other = fixture();
        other.config.current_period.as_mut().unwrap().end = Some("16/11/2026".into());
        let value = grok_snapshot(other, 1).unwrap();
        assert_eq!(value.windows[0].window_seconds, None);
    }

    // Catches: supported omission of the percentage drops valid billing detail
    // or becomes a registry contract error. This varies the recorded fixture
    // according to the parser's optional-field contract, not a live API capture.
    #[test]
    fn grok_missing_percent_preserves_billing_through_policy() {
        let mut response = fixture();
        response.config.credit_usage_percent = None;
        let snapshot = grok_snapshot(response.clone(), 1).unwrap();
        assert_eq!(snapshot.windows, Vec::<QuotaWindow>::new());
        assert_eq!(snapshot.credits, None);
        let mut entry = policy::Entry::default();
        policy::apply(&mut entry, Ok(snapshot), 1, 1000, None);
        assert!(entry.contract_error.is_none());
        assert!(policy::is_fresh(&entry, 2, 1000));
        let detail = entry.snapshot.unwrap().detail.unwrap();
        let QuotaDetail::Grok {
            response: stored, ..
        } = detail
        else {
            panic!("missing Grok billing detail");
        };
        assert_eq!(
            serde_json::to_value(stored).unwrap(),
            serde_json::to_value(response).unwrap()
        );
    }

    // Catches: treating an invalid reported percent like an absent optional field.
    #[test]
    fn grok_invalid_percent_remains_a_contract_error() {
        for percent in [-1.0, f64::NAN, f64::INFINITY] {
            let mut response = fixture();
            response.config.credit_usage_percent = Some(percent);
            assert!(matches!(
                grok_snapshot(response, 1),
                Err(QuotaError::Contract(_))
            ));
        }
    }

    // Catches: an unknown period invents a duration or labels a percent as credits.
    #[test]
    fn grok_unknown_period_has_a_neutral_usage_label() {
        let mut response = fixture();
        response.config.current_period = None;
        let snapshot = grok_snapshot(response, 1).unwrap();
        assert_eq!(snapshot.windows[0].kind, WindowKind::Other);
        assert_eq!(snapshot.windows[0].label, "usage");
        assert_eq!(snapshot.windows[0].window_seconds, None);
        assert_eq!(snapshot.windows[0].used_percent, 26.0);
    }

    // Catches: an expired Grok login retried as a network failure on every
    // backoff instead of waiting for a new login.
    #[test]
    fn grok_cli_failures_map_to_registry_classes() {
        assert!(matches!(
            classify("Grok billing failed: Invalid or expired credentials"),
            QuotaError::Auth(_)
        ));
        assert!(matches!(
            classify("Failed to parse Grok billing response: missing field"),
            QuotaError::Contract(_)
        ));
        assert!(matches!(
            classify("Provider usage request timed out after 20s"),
            QuotaError::Network(_)
        ));
    }

    // Catches: the billing request runs against the host's default Grok home
    // instead of the session's GROK_HOME, so another account's quota shows.
    // Also: a named profile or an absent login reads the default account.
    #[cfg(unix)]
    #[tokio::test]
    async fn grok_key_reads_billing_from_its_own_home() {
        let home = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let body = serde_json::to_string(&fixture()).unwrap();
        let script = crate::test_support::fake_ssh_script(
            "2999-grok-billing",
            &format!(
                r#"[ "$1 $2" = "agent stdio" ] || exit 2
while IFS= read -r line; do
  case "$line" in
    *'"id":1'*)
      if [ "$GROK_HOME" = '{home}' ]; then
        printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{body}}}'
      else
        printf '%s\n' '{{"jsonrpc":"2.0","id":1,"error":{{"code":-32603,"message":"wrong home"}}}}'
      fi ;;
  esac
done"#,
                home = home.path().display()
            ),
            "exit /b 2",
        );
        let registry = QuotaRegistry::default();
        registry.register_owner(Arc::new(GrokCli::with_program(script)));
        // A home without a login is never read, even with the CLI present.
        let empty = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let missing = SubscriptionKey::new("grok-cli", "xai", empty.path().to_string_lossy());
        let view = registry.ensure(&missing).await;
        assert!(
            matches!(view.state, QuotaState::Unavailable { ref reason } if reason == "login not found"),
            "{view:?}"
        );
        let key = SubscriptionKey::new("grok-cli", "xai", home.path().to_string_lossy());
        // Content is irrelevant: TUIC never reads it.
        std::fs::write(home.path().join("auth.json"), "{}").unwrap();
        let view = registry.ensure(&key).await;
        let snapshot = view
            .snapshot
            .clone()
            .unwrap_or_else(|| panic!("billing from the key's home: {view:?}"));
        assert_eq!(snapshot.windows[0].used_percent, 26.0);
        let mut named = key.clone();
        named.profile = Some("work".into());
        let view = registry.ensure(&named).await;
        assert!(view.snapshot.is_none());
        assert!(
            matches!(view.state, QuotaState::Unavailable { ref reason } if reason == "profile not supported"),
            "{view:?}"
        );
    }
}
