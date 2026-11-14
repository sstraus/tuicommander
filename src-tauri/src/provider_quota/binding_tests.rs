use super::*;

use super::binding::resolve;

#[test]
fn ego_session_never_binds_to_a_cli_credential() {
    let key = resolve(
        "ego",
        &[
            ("HOME", "/home/user"),
            ("EGO_HOME", "/ego"),
            ("CLAUDE_CONFIG_DIR", "/claude"),
            ("CODEX_HOME", "/codex"),
        ],
        Some("openai-codex"),
        None,
    )
    .unwrap();
    assert_eq!(key, SubscriptionKey::new("ego", "openai-codex", "/ego"));
}

#[test]
fn sessions_with_same_config_dir_share_one_key() {
    let a = resolve(
        "claude",
        &[("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "/private")],
        None,
        None,
    )
    .unwrap();
    let b = resolve(
        "claude",
        &[("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "/private")],
        None,
        None,
    )
    .unwrap();
    assert_eq!(a, b);
}

#[test]
fn different_config_dirs_get_two_keys() {
    let a = resolve(
        "claude",
        &[("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "/first")],
        None,
        None,
    )
    .unwrap();
    let b = resolve(
        "claude",
        &[("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "/second")],
        None,
        None,
    )
    .unwrap();
    assert_ne!(a, b);
}

#[tokio::test]
async fn missing_cli_marks_only_its_own_keys_unavailable() {
    struct MissingCli;
    impl QuotaOwner for MissingCli {
        fn owner(&self) -> &'static str {
            "claude-code"
        }
        fn read<'a>(
            &'a self,
            _: &'a SubscriptionKey,
        ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
            Box::pin(async { Err(QuotaError::Unsupported) })
        }
    }
    let registry = QuotaRegistry::default();
    registry.register_owner(std::sync::Arc::new(MissingCli));
    let claude = resolve("claude", &[("HOME", "/home/user")], None, None).unwrap();
    let ego = resolve(
        "ego",
        &[("HOME", "/home/user"), ("EGO_HOME", "/ego")],
        Some("anthropic"),
        None,
    )
    .unwrap();
    registry.bind("claude", claude.clone());
    registry.bind("ego", ego.clone());
    let value = QuotaSnapshot {
        windows: vec![],
        detail: None,
        plan: None,
        credits: None,
        account: Some("ego-account".into()),
        observed_at_ms: u64::MAX,
        source: QuotaSource::Passive,
    };
    assert!(registry.observe(&ego, value.clone()));
    let claude_view = registry.ensure(&claude).await;
    assert!(matches!(claude_view.state, QuotaState::Unavailable { .. }));
    assert_eq!(registry.get(&ego).snapshot, Some(value));
    assert_eq!(registry.binding("ego"), Some(ego));
}

#[test]
fn codex_profile_changes_only_the_codex_key() {
    let env = [("HOME", "/home/user"), ("CODEX_HOME", "/codex")];
    let default = resolve("codex", &env, None, None).unwrap();
    let profile = resolve("codex", &env, None, Some("work")).unwrap();
    assert_ne!(default, profile);
    assert_eq!(profile.credential, "/codex");
    assert_eq!(profile.profile.as_deref(), Some("work"));
}

#[test]
fn missing_ego_provider_never_falls_back_to_cli() {
    let env = [("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "/claude")];
    assert_eq!(resolve("ego", &env, None, None), None);
}

#[test]
fn external_api_credentials_do_not_use_claude_login() {
    assert_eq!(
        resolve(
            "claude",
            &[("HOME", "/home/user"), ("ANTHROPIC_API_KEY", "present")],
            None,
            None
        ),
        None
    );
}

#[test]
fn relative_credential_directory_is_not_guessed() {
    assert_eq!(
        resolve(
            "claude",
            &[("HOME", "/home/user"), ("CLAUDE_CONFIG_DIR", "relative")],
            None,
            None
        ),
        None
    );
}
