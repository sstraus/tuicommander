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

/// Reconcile only live PTY agents. A missing process/environment is not a
/// license to substitute the host's credentials.
pub(super) async fn reconcile(state: Arc<crate::AppState>) {
    let snapshot: Vec<(String, Option<String>)> = state
        .session_maps
        .session_states
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().agent_type.clone()))
        .collect();
    super::pi::retain_sessions(&snapshot.iter().map(|(id, _)| id.as_str()).collect());
    let scan_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        snapshot
            .into_iter()
            .map(|(id, agent)| {
                let pid = crate::pty::session_leaf_pid(&scan_state, &id);
                let key = agent.as_deref().and_then(|agent| {
                    let pid = pid?;
                    let env = crate::process_env::read_process_environment(pid).ok()?;
                    let argv = if agent == "codex" || agent == "ego" {
                        Some(crate::process_env::read_process_argv(pid)?)
                    } else {
                        None
                    };
                    let codex = if agent == "codex" {
                        Some(crate::session_usage::codex_profile(&env, argv.as_ref()?).ok()?)
                    } else {
                        None
                    };
                    if agent == "ego"
                        && argv
                            .as_ref()?
                            .iter()
                            .any(|arg| arg == "--session-dir" || arg.starts_with("--session-dir="))
                    {
                        return None;
                    }
                    let env: Vec<_> = env.iter().filter_map(|v| v.split_once('=')).collect();
                    let provider = if agent == "ego" {
                        let (home, session) = crate::agent_session::ego_chat_session_for_pid(pid)?;
                        crate::agent_session::ego_session_provider(&home.join("sessions"), &session)
                    } else if agent == "pi" {
                        super::pi::session_provider(&id)
                    } else {
                        None
                    };
                    let profile = codex
                        .as_ref()
                        .and_then(|(_, _, profile)| profile.as_deref());
                    resolve(agent, &env, provider.as_deref(), profile)
                });
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
