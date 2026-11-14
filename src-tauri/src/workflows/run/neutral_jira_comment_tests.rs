use super::neutral_jira_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::jira::*;
use serde_json::{Value, json};

// Catches: assuming GET issue comments expands properties, stopping at page one,
// or creating a second comment to recover the original lost response.
// Constructed properties fixtures from Jira Cloud REST API v3, inspected 2026-11-11:
// https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-comments/#api-rest-api-3-comment-list-post
// GET pages replay real cloud.json; POST comment/list owns the property expansion.
#[tokio::test]
async fn comment_recovery_observes_original_property_across_bounded_pages() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let first = record["commentPages"][0].clone();
    let second = record["commentPages"][1].clone();
    let first_id = first["comments"][0]["id"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let second_id = second["comments"][0]["id"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    for unique in [true, false] {
        let mut first_details = first["comments"][0].clone();
        let mut second_details = second["comments"][0].clone();
        first_details["properties"] = if unique {
            json!([])
        } else {
            json!([{"key":"tuic.workflow.effect","value":"<effect>"}])
        };
        second_details["properties"] = json!([{"key":"tuic.workflow.effect","value":"<effect>"}]);
        let expected = second_details["id"].clone();
        let details =
            |v: Value| json!({"startAt":0,"maxResults":1000,"total":1,"isLast":true,"values":[v]});
        let (f, effect, task, requests) = recovery(
            dir.path(),
            JiraRequest::Comment {
                issue: reference(),
                body: "Original".into(),
            },
            vec![
                record["issue"].clone(),
                first.clone(),
                details(first_details),
                second.clone(),
                details(second_details),
            ],
        )
        .await;
        let result = f.host.reconcile(effect).await;
        task.abort();
        let _ = task.await;
        if unique {
            assert_eq!(outputs(result.unwrap())["comment_id"], expected);
        } else {
            assert!(result.unwrap_err().contains("not unique"));
        }
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests[1].starts_with("GET ") && !requests[1].contains("expand=properties"));
        assert!(requests[3].contains("startAt=1") && !requests[3].contains("expand=properties"));
        for (index, id) in [(2, first_id), (4, second_id)] {
            assert!(
                requests[index].starts_with("POST /api/rest/api/3/comment/list?expand=properties")
            );
            let body: Value =
                serde_json::from_str(requests[index].split_once('\n').unwrap().1).unwrap();
            assert_eq!(body, json!({"ids":[id]}));
        }
        assert!(
            requests
                .iter()
                .filter(|r| r.starts_with("POST "))
                .all(|r| r.starts_with("POST /api/rest/api/3/comment/list?"))
        );
    }
}
