use super::neutral_jira_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::jira::*;
use serde_json::json;

// Catches: creating a replacement issue instead of finding the original effect,
// or accepting an unrelated/multiple match as the original create receipt.
#[tokio::test]
async fn create_recovery_requires_unique_original_marker_and_only_observes() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let mut issue = cloud()["issue"].clone();
    // Constructed label echo for the documented create/search/read contract.
    issue["fields"]["labels"] = json!(["tuic-effect-<effect>"]);
    let mut fields = fields();
    fields.summary = Some("Original create".into());
    for unique in [true, false] {
        let issues = if unique {
            vec![issue.clone()]
        } else {
            vec![issue.clone(), issue.clone()]
        };
        let mut replies = vec![json!({"issues":issues,"isLast":true})];
        if unique {
            replies.push(issue.clone());
        }
        let (f, effect, task, requests) = recovery(
            dir.path(),
            JiraRequest::Create {
                issue_type_id: "10006".into(),
                fields: fields.clone(),
            },
            replies,
        )
        .await;
        f.writes.store(false, std::sync::atomic::Ordering::Release);
        let result = f.host.reconcile(effect.clone()).await;
        task.abort();
        let _ = task.await;
        if unique {
            assert_eq!(
                outputs(result.unwrap())["issue"]["reference"]["id"],
                "10004"
            );
        } else {
            assert!(result.unwrap_err().contains("not uniquely observed"));
        }
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), if unique { 2 } else { 1 });
        assert!(requests[0].starts_with("POST /api/rest/api/3/search/jql"));
        assert!(requests[0].contains(&effect.key.digest));
        assert!(
            requests
                .iter()
                .all(|r| !r.starts_with("POST /api/rest/api/3/issue"))
        );
    }
}

// Catches: allowing stale transitions or metadata-forbidden edits to send a write,
// and accepting a foreign project's issue as a configured resource.
#[tokio::test]
async fn stale_transition_and_uneditable_fields_do_not_send_mutation() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    for transition in [true, false] {
        let mut fields = fields();
        fields.summary = Some("New summary".into());
        let request = if transition {
            JiraRequest::Transition {
                issue: reference(),
                transition_id: "gone".into(),
                target_status_id: "10005".into(),
                fields: fields.clone(),
            }
        } else {
            JiraRequest::Update {
                issue: reference(),
                fields,
            }
        };
        let metadata = if transition {
            record["transitions"].clone()
        } else {
            json!({"fields":{"summary":{"operations":[]}}})
        };
        let (state, base, server) = server(
            dir.path(),
            vec![(200, record["issue"].clone(), None), (200, metadata, None)],
        )
        .await;
        let f = fixture(state, &base, request);
        assert!(f.host.invoke_effect(claimed(&f)).await.is_err());
        let requests = server.await.unwrap();
        assert!(requests.iter().all(|r| r.starts_with("GET ")));
    }
    let mut foreign = record["issue"].clone();
    foreign["fields"]["project"]["id"] = json!("99999");
    let (state, base, server) = server(dir.path(), vec![(200, foreign, None)]).await;
    let f = fixture(state, &base, JiraRequest::Lookup { issue: reference() });
    assert!(
        f.host
            .invoke_effect(claimed(&f))
            .await
            .unwrap_err()
            .contains("outside installed project")
    );
    server.await.unwrap();
}
