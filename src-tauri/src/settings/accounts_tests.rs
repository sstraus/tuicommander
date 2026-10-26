use super::accounts::{GithubAction, GithubSettings};
use crate::{credentials::Credential, github_account::*};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn isolated() -> (tempfile::TempDir, impl Drop, Arc<crate::state::AppState>) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    (dir, guard, state)
}

// Catches: cloud PAT credentials bypassing the GitHub device-login owner flow.
#[tokio::test]
async fn cloud_pat_cannot_bypass_login() {
    let (_dir, _guard, state) = isolated();
    let settings = GithubSettings::new(state.clone());
    let error = settings
        .perform(GithubAction::AddEnterprise {
            host: "https://github.com".into(),
            label: "cloud".into(),
            token: "cloud-pat-canary".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "auth_required");
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("cloud-pat-canary")
    );
    assert_eq!(GitHubAccountRegistry::load().list(), []);
}

// Catches: account removal leaving bindings, credentials, cooldowns and viewer caches alive.
#[tokio::test]
async fn account_remove_cleans_cache_and_bindings() {
    let (dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(
        GitHubHost::new("ghe.remove.example").unwrap(),
        Some("viewer".into()),
    );
    add_account_record(account.clone(), Some("remove-pat-canary")).unwrap();
    let mut bindings = RepoBindingStore::load();
    bindings.set_binding(
        dir.path(),
        RepoBinding {
            account_id: account.id.clone(),
            owner: "team".into(),
            repo: "project".into(),
            remote_name: "origin".into(),
        },
    );
    bindings.save(&RepoBindingStore::default()).unwrap();
    state
        .github
        .ghe_state
        .insert(account.id.clone(), crate::github::GheAccountState::new());
    state
        .github
        .ghe_state
        .insert("other".into(), crate::github::GheAccountState::new());
    state.git_cache.github_repo_cooldown.insert(
        format!("{}:team/project", account.id),
        std::time::Instant::now(),
    );
    state
        .git_cache
        .github_repo_cooldown
        .insert("other:team/project".into(), std::time::Instant::now());
    let settings = GithubSettings::new(state.clone());
    settings
        .perform(GithubAction::RemoveAccount {
            id: account.id.clone(),
        })
        .await
        .unwrap();
    assert!(GitHubAccountRegistry::load().get(&account.id).is_none());
    assert!(RepoBindingStore::load().get_binding(dir.path()).is_none());
    assert_eq!(
        crate::credentials::get(Credential::AccountToken(&account.id)).unwrap(),
        None
    );
    assert!(!state.github.ghe_state.contains_key(&account.id));
    assert!(
        !state
            .git_cache
            .github_repo_cooldown
            .contains_key(&format!("{}:team/project", account.id))
    );
    assert!(state.github.ghe_state.contains_key("other"));
    assert!(
        state
            .git_cache
            .github_repo_cooldown
            .contains_key("other:team/project")
    );
    assert_eq!(settings.read().await.unwrap()["bindings"], json!([]));
}

// Catches: returning vault/ambient token values rather than presence and real provenance.
#[tokio::test]
async fn github_secret_reads_return_presence_only() {
    let (_dir, _guard, state) = isolated();
    let account = GitHubAccount::github_com_named("named-viewer");
    add_account_record(account, Some("named-oauth-canary")).unwrap();
    *state.github.token.write() = Some("ambient-env-canary".into());
    *state.github.token_source.write() = crate::github_auth::TokenSource::Env;
    let settings = GithubSettings::new(state.clone());
    let read = settings.read().await.unwrap();
    let accounts = read["accounts"].as_array().unwrap();
    let named = accounts.iter().find(|a| a["id"] == "named-viewer").unwrap();
    assert_eq!(named["secret"], json!({"status":"set"}));
    assert_eq!(read["auth"]["source"], "env");
    assert_eq!(read["auth"]["secret"], json!({"status":"set"}));
    let bytes = serde_json::to_string(&read).unwrap();
    assert!(!bytes.contains("named-oauth-canary"));
    assert!(!bytes.contains("ambient-env-canary"));
    assert!(read["revision"].is_string());
    assert_ne!(read["revision"], Value::Null);
}

// Catches: owner edits reporting success without durable label readback, or modifying ambient identity.
#[tokio::test]
async fn label_edit_reads_back_and_cloud_secret_replacement_is_refused() {
    let (_dir, _guard, state) = isolated();
    add_account_record(
        GitHubAccount::github_com_named("label-viewer"),
        Some("label-token-canary"),
    )
    .unwrap();
    let settings = GithubSettings::new(state.clone());
    let reply = settings
        .perform(GithubAction::SetLabel {
            id: "label-viewer".into(),
            label: "Personal account".into(),
        })
        .await
        .unwrap();
    let reread = settings.read().await.unwrap();
    assert_eq!(reply["revision"], reread["revision"]);
    let named = reread["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "label-viewer")
        .unwrap();
    assert_eq!(named["label"], "Personal account");
    assert_eq!(named["login"], "label-viewer");
    let error = settings
        .perform(GithubAction::SetSecret {
            id: "label-viewer".into(),
            token: "replacement-pat-canary".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "auth_required");
    assert_eq!(
        crate::credentials::get(Credential::AccountToken("label-viewer"))
            .unwrap()
            .as_deref(),
        Some("label-token-canary")
    );
    assert!(
        !serde_json::to_string(&reply)
            .unwrap()
            .contains("label-token-canary")
    );
}

// Catches: raw bindings accepting an arbitrary repo/host or stale revision and losing an owner edit.
#[tokio::test]
async fn binding_uses_registered_remote_and_stale_revision_writes_nothing() {
    let (dir, _guard, state) = isolated();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", "git@github.com:team/project.git"],
    ] {
        let result = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let path = repo.to_str().unwrap().to_owned();
    let settings = GithubSettings::new(state.clone());
    let revision = settings.read().await.unwrap()["bindingsRevision"]
        .as_str()
        .unwrap()
        .to_owned();
    let bind = || GithubAction::BindRepo {
        repo_path: path.clone(),
        account_id: "github.com".into(),
        remote_name: "origin".into(),
        expected_revision: revision.clone(),
    };
    assert!(settings.perform(bind()).await.is_err());
    assert_eq!(settings.read().await.unwrap()["bindings"], json!([]));
    crate::config::replace_repositories_for_test(
        json!({"repos": {path.clone(): {}}, "groups":[], "order":[path.clone()]}),
    )
    .unwrap();
    let reply = settings.perform(bind()).await.unwrap();
    let binding = &reply["snapshot"]["bindings"][0];
    assert_eq!(binding["owner"], "team");
    assert_eq!(binding["repo"], "project");
    assert_eq!(binding["account_id"], "github.com");
    let error = settings
        .perform(GithubAction::UnbindRepo {
            repo_path: path.clone(),
            expected_revision: revision,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "conflict");
    assert_eq!(
        settings.read().await.unwrap()["bindings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let revision = settings.read().await.unwrap()["bindingsRevision"]
        .as_str()
        .unwrap()
        .to_owned();
    add_account_record(
        GitHubAccount::ghe_pat(GitHubHost::new("different.example").unwrap(), None),
        None,
    )
    .unwrap();
    assert!(
        settings
            .perform(GithubAction::BindRepo {
                repo_path: path.clone(),
                account_id: "different.example".into(),
                remote_name: "origin".into(),
                expected_revision: revision.clone()
            })
            .await
            .is_err()
    );
    let reply = settings
        .perform(GithubAction::UnbindRepo {
            repo_path: path,
            expected_revision: revision,
        })
        .await
        .unwrap();
    assert_eq!(reply["snapshot"]["bindings"], json!([]));
}

// Catches: mistaking unreadable vault credentials for unset or exposing provider errors.
#[tokio::test]
async fn unreadable_secret_is_unavailable_and_unsafe_enterprise_input_is_not_echoed() {
    let (_dir, _guard, state) = isolated();
    add_account_record(
        GitHubAccount::github_com_named("unreadable"),
        Some("vault-canary"),
    )
    .unwrap();
    let settings = GithubSettings::new(state.clone());
    crate::credentials::MOCK_FAIL_READS.store(true, std::sync::atomic::Ordering::SeqCst);
    let result = settings.read().await;
    crate::credentials::reset_test_faults();
    let read = result.unwrap();
    let row = read["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "unreadable")
        .unwrap();
    assert_eq!(row["secret"], json!({"status":"unavailable"}));
    let error = settings
        .perform(GithubAction::AddEnterprise {
            host: "https://unsafe.example/path?token=input-canary".into(),
            label: "enterprise".into(),
            token: "pat-canary\n".into(),
        })
        .await
        .unwrap_err();
    let text = serde_json::to_string(&error).unwrap();
    assert!(!text.contains("input-canary"));
    assert!(!text.contains("pat-canary"));
}
