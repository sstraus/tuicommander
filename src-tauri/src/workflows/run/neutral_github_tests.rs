use super::RunStore;
use crate::workflows::{github_event::from_rest, github_trigger::GithubResource};
use serde_json::json;

// Catches: comment/update timestamps minting issue work, label removal/re-add
// losing its admission epoch, and late snapshots reverting newer material.
#[test]
fn issue_generations_track_material_and_label_epochs_not_comment_noise() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
    let mut raw: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap();
    let snapshot = from_rest("Octo-Org/Example", GithubResource::Issue, &raw).unwrap();
    let original = store
        .normalize_github(snapshot.clone(), Some("webhook-a".into()))
        .unwrap();
    assert_eq!(original.len(), 2);
    assert_eq!(original[0].event, "opened");
    assert_eq!(original[1].values["label"], json!("bug"));
    assert!(
        original[0].actor.is_none(),
        "issue author is not the update actor"
    );
    assert_eq!(
        store.normalize_github(snapshot, None).unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
    assert_eq!(
        store
            .retained_github_events("octo-org/example", GithubResource::Issue, false)
            .unwrap(),
        original,
        "poll and delivery retain the same original generation"
    );
    raw["updated_at"] = json!("2026-11-11T10:00:01Z");
    raw["comments"] = json!(99);
    let noise = store
        .normalize_github(
            from_rest("octo-org/example", GithubResource::Issue, &raw).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(noise, [] as [crate::workflows::trigger::TriggerEvent; 0]);
    raw["updated_at"] = json!("2026-11-11T10:00:02Z");
    raw["labels"] = json!([]);
    assert_eq!(
        store
            .normalize_github(
                from_rest("octo-org/example", GithubResource::Issue, &raw).unwrap(),
                None
            )
            .unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
    raw["updated_at"] = json!("2026-11-11T10:00:03Z");
    raw["labels"] = json!([{"name":"bug"}]);
    let readded = store
        .normalize_github(
            from_rest("octo-org/example", GithubResource::Issue, &raw).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(readded.len(), 1);
    assert_eq!(readded[0].event, "labeled");
    assert_ne!(readded[0].generation, original[0].generation);
    raw["updated_at"] = json!("2026-11-11T10:00:04Z");
    raw["body"] = json!("Updated requirements");
    let material = store
        .normalize_github(
            from_rest("octo-org/example", GithubResource::Issue, &raw).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(material[0].event, "updated");
    assert_ne!(material[0].generation, readded[0].generation);
    let old = from_rest(
        "octo-org/example",
        GithubResource::Issue,
        &serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        store.normalize_github(old, None).unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
}

// Catches: treating PRs as ordinary issues, trusting branch names instead of
// head/base SHAs, failing to create a reopened generation, or CI-only changes
// restarting a workflow for the same evaluated artifact.
#[test]
fn pr_generations_require_exact_head_and_base_and_ignore_unrelated_ci_updates() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
    let mut raw: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/github-pr-rest.json")).unwrap();
    let first = store
        .normalize_github(
            from_rest("octo-org/example", GithubResource::PullRequest, &raw).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(first[0].values["head"], json!("a".repeat(40)));
    assert_eq!(first[0].values["base"], json!("f".repeat(40)));
    raw["updated_at"] = json!("2026-11-11T10:00:01Z");
    raw["checks_status"] = json!("success");
    assert_eq!(
        store
            .normalize_github(
                from_rest("octo-org/example", GithubResource::PullRequest, &raw).unwrap(),
                None
            )
            .unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
    raw["updated_at"] = json!("2026-11-11T10:00:02Z");
    raw["base"]["sha"] = json!("e".repeat(40));
    let updated = store
        .normalize_github(
            from_rest("octo-org/example", GithubResource::PullRequest, &raw).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(updated[0].event, "updated");
    assert_ne!(updated[0].generation, first[0].generation);
    raw["updated_at"] = json!("2026-11-11T10:00:03Z");
    raw["state"] = json!("closed");
    assert_eq!(
        store
            .normalize_github(
                from_rest("octo-org/example", GithubResource::PullRequest, &raw).unwrap(),
                None
            )
            .unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
    raw["updated_at"] = json!("2026-11-11T10:00:04Z");
    raw["state"] = json!("open");
    assert_eq!(
        store
            .normalize_github(
                from_rest("octo-org/example", GithubResource::PullRequest, &raw).unwrap(),
                None
            )
            .unwrap()[0]
            .event,
        "reopened"
    );
    raw["head"]["sha"] = json!("branch-name");
    assert!(from_rest("octo-org/example", GithubResource::PullRequest, &raw).is_err());
    raw["pull_request"] = json!({});
    assert!(from_rest("octo-org/example", GithubResource::Issue, &raw).is_err());
}

// Catches: enabled intake silently claiming success without the installed native
// catalog, or reaching credentials/network before that authoritative refusal.
#[tokio::test]
async fn github_source_refuses_missing_installed_catalog_before_network() {
    let f = super::neutral_trigger_fixture::fixture();
    f.enable(f.draft.clone(), 0);
    // WorkflowStore retains a WAL connection: copying only the main file loses
    // committed bindings. VACUUM INTO makes a consistent standalone fixture.
    rusqlite::Connection::open(f._dir.path().join("definitions.sqlite3"))
        .unwrap()
        .execute(
            "VACUUM INTO ?1",
            [f._dir.path().join("workflows.sqlite3").to_str().unwrap()],
        )
        .unwrap();
    let _config = crate::config::set_config_dir_override(f._dir.path().to_path_buf());
    let persisted = crate::workflows::WorkflowStore::open()
        .unwrap()
        .all_neutral_trigger_bindings()
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert!(persisted[0].enabled);
    let state = crate::state::tests_support::make_test_app_state();
    let error =
        crate::workflows::github_poll::observe(&state, "/unregistered", GithubResource::Issue)
            .await
            .unwrap_err();
    assert!(error.contains("catalog not installed"), "{error}");
}
