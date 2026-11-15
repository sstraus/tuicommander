use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome};
use super::neutral_jira_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::jira::*;
use serde_json::{Value, json};

// Catches: repeating create after a lost response or releasing the original
// resource merely because eventually consistent search returned no match.
#[tokio::test]
async fn lost_create_response_retains_original_effect_and_never_repeats_create() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    // Constructed fault: server committed but its successful receipt was lost.
    let (state, base, server) = server(
        dir.path(),
        vec![
            (200, record["createmeta"].clone(), None),
            (201, json!({}), None),
            (200, json!({"issues":[],"isLast":true}), None),
        ],
    )
    .await;
    let mut data = fields();
    data.summary = Some("Lost receipt".into());
    let f = fixture(
        state,
        &base,
        JiraRequest::Create {
            issue_type_id: "10006".into(),
            fields: data,
        },
    );
    let effect = claimed(&f);
    assert!(
        f.host
            .invoke_effect(effect.clone())
            .await
            .unwrap_err()
            .contains("missing id")
    );
    let retained = f
        .owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: EffectObservation {
                    stopped: false,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "response lost".into(),
                    },
                },
            },
        )
        .unwrap();
    let retained = retained.effect(&effect.key).unwrap();
    assert!(!retained.settled());
    assert_eq!(retained.key, effect.key);
    assert!(
        f.host
            .reconcile(retained.clone())
            .await
            .unwrap_err()
            .contains("not uniquely observed")
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[2].starts_with("POST /api/rest/api/3/search/jql "));
    assert!(requests[2].contains(&effect.key.digest));
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/rest/api/3/issue "))
            .count(),
        1
    );
    assert!(
        !f.owner
            .store
            .neutral_execution(&effect.key.fence.run)
            .unwrap()
            .effect(&effect.key)
            .unwrap()
            .settled()
    );
}

// Catches: sending fields absent from current edit metadata, confusing Cloud
// accountId with user/name, or accepting update without authoritative read-back.
#[tokio::test]
async fn update_uses_edit_metadata_and_reconciles_expanded_reference_fields() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let mut changed = record["issue"].clone();
    changed["fields"]["summary"] = json!("Fixed summary");
    changed["fields"]["assignee"] =
        json!({"accountId":"recorded-user-1","displayName":"Recorded User"});
    let metadata =
        json!({"fields":{"summary":{"operations":["set"]},"assignee":{"operations":["set"]}}});
    let (state, base, server) = server(
        dir.path(),
        vec![
            (200, record["issue"].clone(), None),
            (200, metadata, None),
            (204, Value::Null, None),
            (200, changed, None),
        ],
    )
    .await;
    let mut data = fields();
    data.summary = Some("Fixed summary".into());
    data.assignee_account_id = Some("recorded-user-1".into());
    let f = fixture(
        state,
        &base,
        JiraRequest::Update {
            issue: reference(),
            fields: data,
        },
    );
    let effect = claimed(&f);
    assert_eq!(
        outputs(f.host.invoke_effect(effect).await.unwrap())["issue"]["fields"]["summary"],
        "Fixed summary"
    );
    let requests = server.await.unwrap();
    let body: Value = serde_json::from_str(requests[2].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"fields":{"summary":"Fixed summary","assignee":{"accountId":"recorded-user-1"}}})
    );
}

// Catches: discarding required custom fields before create, or silently treating
// provider-side metadata restrictions as a successful mutation.
#[tokio::test]
async fn required_custom_fields_block_create_before_post() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let mut metadata = cloud()["createmeta"].clone();
    // Constructed tenant extension from the documented create-field schema.
    metadata["fields"].as_array_mut().unwrap().push(json!({"fieldId":"customfield_19999","required":true,"hasDefaultValue":false,"operations":["set"]}));
    metadata["total"] = json!(metadata["fields"].as_array().unwrap().len());
    let (state, base, server) = server(dir.path(), vec![(200, metadata, None)]).await;
    let mut data = fields();
    data.summary = Some("Missing custom".into());
    let f = fixture(
        state,
        &base,
        JiraRequest::Create {
            issue_type_id: "10006".into(),
            fields: data,
        },
    );
    assert!(
        f.host
            .invoke_effect(claimed(&f))
            .await
            .unwrap_err()
            .contains("customfield_19999")
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET "));
}

// Catches: silently replaying a rate-limited comment, losing Retry-After or
// reporting success while Jira has supplied no comment receipt.
#[tokio::test]
async fn rate_limited_comment_preserves_hint_without_retry() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let (state, base, server) = server(
        dir.path(),
        vec![
            (200, cloud()["issue"].clone(), None),
            (429, json!({}), Some("12")),
        ],
    )
    .await;
    let f = fixture(
        state,
        &base,
        JiraRequest::Comment {
            issue: reference(),
            body: "Comment text".into(),
        },
    );
    let effect = claimed(&f);
    let error = f.host.invoke_effect(effect.clone()).await.unwrap_err();
    assert!(error.contains("Retry-After=12"));
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    let body: Value = serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body["body"]["content"][0]["content"][0]["text"],
        "Comment text"
    );
    assert_eq!(
        body["properties"],
        json!([{"key":"tuic.workflow.effect","value":effect.key.digest}])
    );
}

// Catches: exact JSON equality in read-back. Jira Cloud returns number custom
// fields (story points) as doubles, so a successful update of 5 reads back 5.0
// and the landed write is reported as not observed, forever uncertain.
#[tokio::test]
async fn update_of_number_custom_field_observes_double_read_back() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    let mut changed = record["issue"].clone();
    changed["fields"]["customfield_10016"] = json!(5.0);
    let metadata = json!({"fields":{"customfield_10016":{"operations":["set"]}}});
    let (state, base, server) = server(
        dir.path(),
        vec![
            (200, record["issue"].clone(), None),
            (200, metadata, None),
            (204, Value::Null, None),
            (200, changed, None),
        ],
    )
    .await;
    let mut data = fields();
    data.custom = std::collections::BTreeMap::from([("customfield_10016".into(), json!(5))]);
    let f = fixture(
        state,
        &base,
        JiraRequest::Update {
            issue: reference(),
            fields: data,
        },
    );
    let effect = claimed(&f);
    let observed = f.host.invoke_effect(effect).await;
    server.await.unwrap();
    assert_eq!(
        outputs(observed.expect("landed number update must be observed"))["issue"]["fields"]["customfield_10016"],
        5.0
    );
}
