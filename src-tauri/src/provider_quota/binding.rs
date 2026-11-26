//! Resolve quota identity from the foreground process, not the host process.
use super::SubscriptionKey;
use std::path::Path;
use std::sync::Arc;

pub(super) fn resolve(
    agent: &str,
    env: &[(&str, &str)],
    provider: Option<&str>,
    profile: Option<&str>,
) -> Option<SubscriptionKey> {
    let get = |name: &str| {
        env.iter().find_map(|(key, value)| {
            let matches = if cfg!(windows) {
                key.eq_ignore_ascii_case(name)
            } else {
                *key == name
            };
            (matches && !value.is_empty()).then_some(*value)
        })
    };
    let dir = |variable: &str, default: &str| -> Option<String> {
        if let Some(value) = get(variable) {
            return Path::new(value).is_absolute().then(|| value.to_owned());
        }
        let home = get("HOME").or_else(|| get("USERPROFILE"))?;
        if !Path::new(home).is_absolute() {
            return None;
        }
        Some(Path::new(home).join(default).to_string_lossy().into_owned())
    };
    match agent {
        "claude" if get("ANTHROPIC_API_KEY").is_some() || get("ANTHROPIC_AUTH_TOKEN").is_some() => {
            None
        }
        "claude" => Some(SubscriptionKey::new(
            "claude-code",
            "anthropic",
            dir("CLAUDE_CONFIG_DIR", ".claude")?,
        )),
        "codex" => {
            let mut key =
                SubscriptionKey::new("codex-cli", "openai-codex", dir("CODEX_HOME", ".codex")?);
            key.profile = profile.filter(|p| !p.is_empty()).map(str::to_owned);
            Some(key)
        }
        "grok" => Some(SubscriptionKey::new(
            "grok-cli",
            "xai",
            dir("GROK_HOME", ".grok")?,
        )),
        "ego" => Some(SubscriptionKey::new(
            "ego",
            provider.filter(|p| !p.is_empty())?,
            dir("EGO_HOME", ".ego")?,
        )),
        // The provider is the one of the newest response pi forwarded.
        "pi" => Some(SubscriptionKey::new(
            "pi",
            provider.filter(|p| !p.is_empty())?,
            dir("PI_CODING_AGENT_DIR", ".pi/agent")?,
        )),
        _ => None,
    }
}

/// Why one live agent session has no subscription key. Never holds
/// environment values, only the step that failed.
pub(super) fn agent_key(
    session_id: &str,
    agent: &str,
    pid: Option<u32>,
) -> Result<SubscriptionKey, String> {
    let pid = pid.ok_or("no foreground process")?;
    let env = crate::process_env::read_process_environment(pid)
        .map_err(|_| "process environment unreadable")?;
    let argv = if agent == "codex" || agent == "ego" {
        Some(crate::process_env::read_process_argv(pid).ok_or("process argv unreadable")?)
    } else {
        None
    };
    let codex = match (agent, argv.as_ref()) {
        ("codex", Some(argv)) => Some(crate::session_usage::codex_profile(&env, argv)?),
        _ => None,
    };
    if agent == "ego"
        && argv.as_ref().is_some_and(|argv| {
            argv.iter()
                .any(|arg| arg == "--session-dir" || arg.starts_with("--session-dir="))
        })
    {
        return Err("ego explicit session dir is not bound".into());
    }
    let env: Vec<_> = env.iter().filter_map(|v| v.split_once('=')).collect();
    let (provider, connection) = if agent == "ego" {
        let (home, session) =
            crate::agent_session::ego_chat_session_for_pid(pid).ok_or("ego session unresolved")?;
        let (provider, connection) =
            crate::agent_session::ego_session_quota_identity(&home.join("sessions"), &session)?;
        // The manifest keeps the model route; named quota publications use its vendor.
        // Legacy publications keep their original route spelling.
        let provider = if connection.is_some() {
            match provider.as_str() {
                "openai-codex" => "openai",
                "xai-oauth" => "xai",
                "minimax-oauth" => "minimax",
                provider => provider,
            }
            .to_owned()
        } else {
            provider
        };
        (Some(provider), connection)
    } else if agent == "pi" {
        (super::pi::session_provider(session_id), None)
    } else {
        (None, None)
    };
    let profile = codex
        .as_ref()
        .and_then(|(_, _, profile)| profile.as_deref());
    let mut key = resolve(agent, &env, provider.as_deref(), profile)
        .ok_or("no subscription for this agent environment")?;
    key.connection = connection;
    Ok(key)
}

#[cfg(all(test, unix))]
mod ego_account_tests {
    use super::*;

    // Catches: a live named account reuses another account's or legacy quota key.
    #[test]
    fn ego_selected_accounts_do_not_share_vendor_quota_binding() {
        let home = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let session = "0198c7f4-9c1f-7a2b-8c3d-4e5f60718293";
        let directory = home.path().join("sessions").join(session);
        std::fs::create_dir_all(&directory).unwrap();
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "read -r line"])
            .env("EGO_HOME", home.path())
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // The shell blocks on its owned stdin; no timing sleeps or live ego data.
        let pid = child.id();
        let lease = directory.join("lease");
        std::fs::write(&lease, format!("{{\"pid\":{pid},\"heartbeat\":1}}")).unwrap();
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lease)
            .unwrap();
        lease.lock().unwrap();
        let read = |connection: Option<&str>, model: &str| {
            std::fs::write(
                directory.join("manifest.json"),
                serde_json::json!({"model": model, "connection": connection}).to_string(),
            )
            .unwrap();
            agent_key("quota-account-test", "ego", Some(pid)).unwrap()
        };
        let account_a = read(Some("account_a"), "anthropic/example");
        let account_b = read(Some("account_b"), "anthropic/example");
        let legacy = read(None, "anthropic/example");
        // ego's manifest uses a model route, but named quota publishes the vendor.
        // Catches: named OAuth routes cannot match their vendor-keyed usage observations.
        let routes: Vec<_> = [
            ("openai-codex", "openai"),
            ("xai-oauth", "xai"),
            ("minimax-oauth", "minimax"),
        ]
        .into_iter()
        .map(|(route, vendor)| {
            let model = format!("{route}/example");
            (
                route,
                vendor,
                read(Some("account_b"), &model),
                read(None, &model),
            )
        })
        .collect();
        drop(child.stdin.take());
        child.wait().unwrap();
        assert_ne!(account_a, account_b, "same-vendor accounts share quota");
        assert_ne!(account_b, legacy, "named account can read legacy quota");
        assert_eq!(
            account_b.provider, "anthropic",
            "vendor remains display identity"
        );
        for (route, vendor, named, legacy) in routes {
            assert_eq!(
                named.provider, vendor,
                "named {route} must match vendor quota"
            );
            assert_eq!(
                legacy.provider, route,
                "legacy route identity must stay unchanged"
            );
        }
    }
}

/// Agents `resolve` can bind; a failure of any other agent is not news.
const QUOTA_AGENTS: [&str; 5] = ["claude", "codex", "grok", "ego", "pi"];

/// Sessions whose binding failure was already logged; one line per session.
static LOGGED_FAILURES: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

/// One session's key; the first failure of a quota agent logs its reason.
pub(super) fn scan_session(
    session_id: &str,
    agent: &str,
    pid: Option<u32>,
) -> Option<SubscriptionKey> {
    agent_key(session_id, agent, pid)
        .inspect_err(|reason| {
            if QUOTA_AGENTS.contains(&agent) && LOGGED_FAILURES.lock().insert(session_id.to_owned())
            {
                log_binding_failure(session_id, agent, reason);
            }
        })
        .ok()
}

fn log_binding_failure(session_id: &str, agent: &str, reason: &str) {
    tracing::warn!(
        source = "provider_quota",
        session_id,
        agent,
        reason,
        "Quota binding unavailable"
    );
}

/// Reconcile only live PTY agents. A missing process/environment is not a
/// license to substitute the host's credentials.
pub(super) async fn reconcile(state: Arc<crate::AppState>) {
    let snapshot: Vec<(String, Option<String>)> = state
        .session_maps
        .session_states
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().agent_type.clone()))
        .collect();
    let live = snapshot.iter().map(|(id, _)| id.as_str()).collect();
    super::pi::retain_sessions(&live);
    LOGGED_FAILURES
        .lock()
        .retain(|id| live.contains(id.as_str()));
    let scan_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        snapshot
            .into_iter()
            .map(|(id, agent)| {
                let pid = crate::pty::session_leaf_pid(&scan_state, &id);
                let key = agent
                    .as_deref()
                    .and_then(|agent| scan_session(&id, agent, pid));
                (id, agent, pid, key)
            })
            .collect::<Vec<_>>()
    })
    .await;
    let Ok(bindings) = result else {
        return;
    };
    for (id, agent, pid, key) in bindings {
        if crate::pty::session_leaf_pid(&state, &id) != pid {
            continue;
        }
        let Some(current) = state.session_maps.session_states.get(&id) else {
            continue;
        };
        if current.agent_type != agent {
            continue;
        }
        match key {
            Some(key) if state.provider_quota.binding(&id).as_ref() != Some(&key) => {
                state.provider_quota.bind(&id, key);
            }
            None => {
                state.provider_quota.unbind(&id);
            }
            _ => {}
        }
    }
}
