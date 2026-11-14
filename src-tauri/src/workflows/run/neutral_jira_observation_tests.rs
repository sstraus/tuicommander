use super::neutral_jira_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::jira::*;
use serde_json::{Value, json};

// Catches: rejecting expanded/reordered multiselect options, matching one option
// twice, reordering document content, or collapsing distinct integers through f64.
// Constructed tenant-field read-backs use the documented Cloud v3 field contract.
#[tokio::test]
async fn update_observes_option_expansion_without_losing_order_or_number_precision() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let record = cloud();
    for (wanted, actual, observed) in [
        (
            json!([{"value":"A"},{"id":"202"}]),
            json!([{"id":"202","value":"B","self":"provider-link"},{"id":"201","value":"A"}]),
            true,
        ),
        (
            json!([{"id":"201"},{"id":"201"}]),
            json!([{"id":"201"},{"id":"202"}]),
            false,
        ),
        (
            json!({"type":"doc","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}),
            json!({"type":"doc","content":[{"type":"text","text":"b"},{"type":"text","text":"a"}]}),
            false,
        ),
        (
            json!(9_007_199_254_740_993u64),
            json!(9_007_199_254_740_992.0),
            false,
        ),
        (json!(5), json!(6.0), false),
    ] {
        let mut changed = record["issue"].clone();
        changed["fields"]["customfield_19999"] = actual.clone();
        let metadata = json!({"fields":{"customfield_19999":{"operations":["set"]}}});
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
        let mut fields = fields();
        fields
            .custom
            .insert("customfield_19999".into(), wanted.clone());
        let f = fixture(
            state,
            &base,
            JiraRequest::Update {
                issue: reference(),
                fields,
            },
        );
        let result = f.host.invoke_effect(claimed(&f)).await;
        server.await.unwrap();
        if observed {
            assert_eq!(
                outputs(result.unwrap())["issue"]["fields"]["customfield_19999"],
                actual
            );
        } else {
            assert!(
                result.unwrap_err().contains("not yet observed"),
                "wanted {wanted}, actual {actual}"
            );
        }
    }
}
