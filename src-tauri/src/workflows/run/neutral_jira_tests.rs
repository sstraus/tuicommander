use super::neutral_jira_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::jira::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

// Catches: mapping GitHub issue number into Jira project/type, losing source URL,
// or requiring a Story root to deliver the mapped operation's canonical result.
#[tokio::test]
async fn classified_github_issue_creates_configured_jira_issue_without_story_root() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let issue = crate::github::GitHubIssue {
        number: 73,
        title: "Broken export".into(),
        state: "OPEN".into(),
        url: "https://github.com/owner/repo/issues/73".into(),
        author: "reporter".into(),
        author_association: "NONE".into(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        comments_count: 0,
        created_at: "2026-11-11".into(),
        updated_at: "2026-11-11".into(),
        requirements_digest: "snapshot-digest".into(),
    };
    let request = classified_issue_create(
        &issue,
        "bug",
        &BTreeMap::from([("bug".into(), "10006".into())]),
    )
    .unwrap();
    assert!(classified_issue_create(&issue, "unknown", &BTreeMap::new()).is_err());
    let (state, base, server) = server(
        dir.path(),
        vec![
            (200, record["createmeta"].clone(), None),
            (201, record["create"].clone(), None),
            (200, record["issue"].clone(), None),
        ],
    )
    .await;
    let f = fixture(state, &base, request);
    super::runtime::run_actor(
        std::sync::Arc::downgrade(&f.owner),
        f.invocation.fence.run.clone(),
        std::sync::Arc::new(tokio::sync::Notify::new()),
    )
    .await;
    let snapshot = f
        .owner
        .store
        .neutral_execution(&f.invocation.fence.run)
        .unwrap();
    assert_eq!(
        snapshot.status,
        super::neutral_execution::NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(
        snapshot.outputs.unwrap().values["result"]["issue"]["reference"],
        json!({"account":"fixture","id":"10004"})
    );
    let requests = server.await.unwrap();
    let body: Value = serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["fields"]["summary"], "Broken export");
    assert_eq!(body["fields"]["project"], json!({"id":"10001"}));
    assert_eq!(body["fields"]["issuetype"], json!({"id":"10006"}));
    assert!(
        body["fields"]["description"]
            .to_string()
            .contains("/issues/73")
    );
    assert!(
        body["fields"]["labels"][0]
            .as_str()
            .unwrap()
            .starts_with("tuic-effect-")
    );
}

// Catches: dropping opaque pagination tokens, treating a partial page as complete,
// and accepting a repeated cursor instead of surfacing a stalled provider.
#[tokio::test]
async fn search_returns_recorded_pages_and_rejects_repeated_cursor() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let token = record["searchPages"][0]["nextPageToken"]
        .as_str()
        .unwrap()
        .to_string();
    for (cursor, page, ok) in [
        (None, record["searchPages"][0].clone(), true),
        (Some(token.clone()), record["searchPages"][1].clone(), true),
        (Some(token.clone()), record["searchPages"][0].clone(), false),
    ] {
        let (state, base, server) = server(dir.path(), vec![(200, page, None)]).await;
        let f = fixture(
            state,
            &base,
            JiraRequest::Search {
                jql: "status != Done".into(),
                next_page_token: cursor.clone(),
                limit: 1,
            },
        );
        let result = f.host.invoke_effect(claimed(&f)).await;
        if ok {
            let result = outputs(result.unwrap());
            assert_eq!(result["issues"].as_array().unwrap().len(), 1);
            assert_eq!(
                result["next_page_token"],
                if cursor.is_none() {
                    json!(token)
                } else {
                    record["searchPages"][1]["nextPageToken"].clone()
                }
            );
        } else {
            assert!(result.unwrap_err().contains("repeated"));
        }
        let requests = server.await.unwrap();
        let body: Value =
            serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["jql"], "project = \"10001\" AND (status != Done)");
        if let Some(cursor) = cursor {
            assert_eq!(body["nextPageToken"], cursor);
        }
    }
}

// Catches: a graph overriding installed site/account/operation, a foreign issue,
// or a once-approved binding continuing to write after current grants are revoked.
#[tokio::test]
async fn installed_binding_rejects_foreign_references_override_and_revoked_write() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let f = fixture(
        state,
        "http://127.0.0.1:1/api/",
        JiraRequest::Comment {
            issue: reference(),
            body: "test".into(),
        },
    );
    let effect = claimed(&f);
    let mut invocation = f.invocation.clone();
    invocation
        .instance
        .config
        .insert("connector".into(), json!({"account":"other"}));
    assert!(
        f.host
            .prepare(&invocation)
            .unwrap_err()
            .contains("override")
    );
    let mut foreign = reference();
    foreign.account = "other".into();
    invocation = f.invocation.clone();
    invocation.inputs = inputs(&JiraRequest::Comment {
        issue: foreign,
        body: "test".into(),
    });
    assert!(
        f.host
            .prepare(&invocation)
            .unwrap_err()
            .contains("another account")
    );
    invocation.inputs = inputs(&JiraRequest::Lookup { issue: reference() });
    assert!(
        f.host
            .prepare(&invocation)
            .unwrap_err()
            .contains("operation")
    );
    f.writes.store(false, Ordering::Release);
    assert!(
        f.host
            .invoke_effect(effect)
            .await
            .unwrap_err()
            .contains("grant")
    );
}

// Catches: parsing a successful 204 transition as JSON or confusing transition ID
// 31 with status ID 10005, and accepting read-back from a different state.
#[tokio::test]
async fn transition_checks_legal_id_and_authoritative_status_after_204() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let mut transitions = record["transitions"].clone();
    // Constructed expansion of the documented transitions.fields contract.
    for transition in transitions["transitions"].as_array_mut().unwrap() {
        transition["fields"] = json!({});
    }
    for status in ["10005", "3"] {
        let mut issue = record["issue"].clone();
        issue["fields"]["status"]["id"] = json!(status);
        let (state, base, server) = server(
            dir.path(),
            vec![
                (200, record["issue"].clone(), None),
                (200, transitions.clone(), None),
                (204, Value::Null, None),
                (200, issue, None),
            ],
        )
        .await;
        let f = fixture(
            state,
            &base,
            JiraRequest::Transition {
                issue: reference(),
                transition_id: "31".into(),
                target_status_id: "10005".into(),
                fields: fields(),
            },
        );
        let result = f.host.invoke_effect(claimed(&f)).await;
        if status == "10005" {
            assert_eq!(
                outputs(result.unwrap())["issue"]["fields"]["status"]["id"],
                "10005"
            );
        } else {
            assert!(result.unwrap_err().contains("not yet observed"));
        }
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[2].contains("\"transition\":{\"id\":\"31\"}"));
    }
}

// Catches: GitHub label names copied verbatim into Jira labels. GitHub's default
// labels ("good first issue", "help wanted") contain spaces, which Jira Cloud
// rejects on create, so the classified flow fails for ordinary issues.
#[test]
fn classified_github_issue_with_spaced_label_maps_to_valid_jira_labels() {
    let label = |name: &str| tuic_git::github::PrLabel {
        name: name.into(),
        color: String::new(),
        text_color: String::new(),
        background_color: String::new(),
    };
    let issue = crate::github::GitHubIssue {
        number: 73,
        title: "Crash on start".into(),
        state: "OPEN".into(),
        url: "https://github.com/o/r/issues/73".into(),
        author: "a".into(),
        author_association: "OWNER".into(),
        labels: vec![label("good first issue"), label("bug")],
        assignees: vec![],
        milestone: None,
        comments_count: 0,
        created_at: String::new(),
        updated_at: String::new(),
        requirements_digest: String::new(),
    };
    let types = BTreeMap::from([("bug".to_string(), "10006".to_string())]);
    let request = classified_issue_create(&issue, "bug", &types).unwrap();
    request.validate("fixture").unwrap();
    let JiraRequest::Create { fields, .. } = request else {
        panic!("not a create")
    };
    let labels = fields.labels.unwrap();
    assert_eq!(labels, vec!["good-first-issue", "bug"]);
    assert!(
        labels.iter().all(|l| !l.chars().any(char::is_whitespace)),
        "Jira rejects labels with spaces: {labels:?}"
    );
}
