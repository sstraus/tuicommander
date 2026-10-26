use super::{
    accounts::{GithubAction, GithubSettings},
    accounts_tests::isolated,
};
use crate::{credentials::Credential, github_account::*};

// Catches: removal deleting an account/token before discovering corrupt bindings, then overwriting them.
#[tokio::test]
async fn corrupt_binding_metadata_refuses_removal_without_deleting_credentials() {
    let (dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("corrupt.example").unwrap(), None);
    add_account_record(account.clone(), Some("corrupt-token-canary")).unwrap();
    let path = dir.path().join("github_bindings.json");
    std::fs::write(&path, "corrupt binding bytes").unwrap();
    let settings = GithubSettings::new(state.clone());
    let error = settings
        .perform(GithubAction::RemoveAccount {
            id: account.id.clone(),
        })
        .await
        .unwrap_err();
    assert!(!error.persisted);
    assert!(
        settings
            .perform(GithubAction::RemoveAccount {
                id: account.id.clone()
            })
            .await
            .is_err()
    );
    assert!(GitHubAccountRegistry::load().get(&account.id).is_some());
    assert_eq!(
        crate::credentials::get(Credential::AccountToken(&account.id))
            .unwrap()
            .as_deref(),
        Some("corrupt-token-canary")
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "corrupt binding bytes"
    );
}

// Catches: a published action schema accepting undeclared secret fields or lacking write-only markers.
#[test]
fn typed_action_schema_rejects_unknown_fields_and_marks_tokens_write_only() {
    let schema = GithubSettings::action_schema();
    let variants = schema["oneOf"].as_array().unwrap();
    let secret_variants: Vec<_> = variants
        .iter()
        .filter(|variant| variant["properties"].get("token").is_some())
        .collect();
    assert_eq!(secret_variants.len(), 2);
    for variant in secret_variants {
        assert_eq!(variant["properties"]["token"]["writeOnly"], true);
    }
    assert!(serde_json::from_value::<GithubAction>(serde_json::json!({"action":"remove_account","id":"github.com","token":"unrecognized-canary"})).is_err());
}

// Catches: owner refusals reported as store_failed, so the user retries a request that can never succeed.
#[tokio::test]
async fn owner_refusals_keep_their_cause() {
    let (_dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("refuse.example").unwrap(), None);
    add_account_record(account.clone(), Some("refuse-token-canary")).unwrap();
    let settings = GithubSettings::new(state.clone());
    let cases = [
        (
            GithubAction::RemoveAccount {
                id: GitHubAccount::GITHUB_COM_ID.into(),
            },
            "not_allowed",
        ),
        (
            GithubAction::SetLabel {
                id: account.id.clone(),
                label: " ".into(),
            },
            "out_of_range",
        ),
        (
            GithubAction::AddEnterprise {
                host: "github.com".into(),
                label: "cloud".into(),
                token: "bare-cloud-canary".into(),
            },
            "out_of_range",
        ),
        (
            GithubAction::AddEnterprise {
                host: "https://user@ghe.example".into(),
                label: "userinfo".into(),
                token: "userinfo-canary".into(),
            },
            "out_of_range",
        ),
        (
            GithubAction::AddEnterprise {
                host: "https://other.example".into(),
                label: "split".into(),
                token: "line\nbreak".into(),
            },
            "out_of_range",
        ),
    ];
    for (action, code) in cases {
        let error = settings.perform(action).await.unwrap_err();
        assert_eq!((error.code.as_str(), error.persisted), (code, false));
    }
}

// Catches: bindings-save failure deletes the account, making its dangling binding irreparable by retry.
#[tokio::test]
async fn bindings_save_failure_keeps_account_retriable() {
    let (dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("save-failure.example").unwrap(), None);
    add_account_record(account.clone(), Some("failure-token-canary")).unwrap();
    let bindings = serde_json::json!({"bindings":{"/test/repo":{"account_id":account.id,"owner":"owner","repo":"repo","remote_name":"origin"}}});
    std::fs::write(
        dir.path().join("github_bindings.json"),
        bindings.to_string(),
    )
    .unwrap();
    // A directory at the lock path makes bindings persistence fail after a successful read.
    let lock = dir.path().join("github_bindings.json.lock");
    std::fs::create_dir(&lock).unwrap();
    let settings = GithubSettings::new(state.clone());
    assert!(
        settings
            .perform(GithubAction::RemoveAccount {
                id: account.id.clone()
            })
            .await
            .is_err()
    );
    assert!(
        GitHubAccountRegistry::load().get(&account.id).is_some(),
        "failed bindings save must not delete the account"
    );
    assert_eq!(
        crate::credentials::get(Credential::AccountToken(&account.id))
            .unwrap()
            .as_deref(),
        Some("failure-token-canary")
    );
    std::fs::remove_dir(lock).unwrap();
    settings
        .perform(GithubAction::RemoveAccount {
            id: account.id.clone(),
        })
        .await
        .unwrap();
    assert!(GitHubAccountRegistry::load().get(&account.id).is_none());
    assert_eq!(
        RepoBindingStore::load().entries(),
        [] as [crate::github_account::RepoBindingEntry; 0]
    );
}

// Catches: credential actions missing despite account metadata coverage.
#[test]
fn github_credential_actions_are_catalogued() {
    let catalogue = super::native::native_catalogue().unwrap();
    let field = &catalogue.fields["github_accounts.accounts.{id}.credential"];
    assert_eq!(field.path, "/accounts/{id}/credential");
    assert!(field.default.is_none());
    for operation in [
        super::model::Operation::Get,
        super::model::Operation::SetSecret,
        super::model::Operation::ClearSecret,
    ] {
        assert!(field.annotation.operations.contains(&operation));
    }
    let schema = GithubSettings::action_schema().to_string();
    for action in [
        "begin_cloud_login",
        "complete_cloud_login",
        "add_enterprise",
        "set_secret",
        "clear_secret",
        "remove_account",
    ] {
        assert!(schema.contains(action));
    }
}

// Catches: provider or vault errors exposing credentials.
#[tokio::test]
async fn github_credential_action_failure_is_redacted() {
    let (_dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("vault-failure.example").unwrap(), None);
    add_account_record(account.clone(), Some("original-token-canary")).unwrap();
    let settings = GithubSettings::new(state);
    crate::credentials::MOCK_FAIL_WRITES.store(true, std::sync::atomic::Ordering::SeqCst);
    let result = settings
        .perform(GithubAction::ClearSecret {
            id: account.id.clone(),
        })
        .await;
    crate::credentials::reset_test_faults();
    let error = result.unwrap_err();
    assert!(!error.persisted);
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("original-token-canary")
    );
    assert_eq!(
        crate::credentials::get(Credential::AccountToken(&account.id))
            .unwrap()
            .as_deref(),
        Some("original-token-canary")
    );
}
