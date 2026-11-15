use super::*;
use serde_json::json;
fn recorded() -> Value {
    serde_json::from_str(include_str!(
        "../../../../plugins/gitlab/fixtures/recorded.json"
    ))
    .unwrap()
}
fn documented() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/gitlab-workflow/documented.json"
    ))
    .unwrap()
}
fn install() -> GitLabInstall {
    GitLabInstall::new(
        "https://gitlab.example.invalid:8443/api/v4/".into(),
        1,
        "account".into(),
    )
    .unwrap()
}
fn values(value: Value) -> BTreeMap<String, Value> {
    serde_json::from_value(value).unwrap()
}

// Catches: returning another project/subject, or treating a failed pipeline on a
// different head as evidence that the selected artifact passed.
#[test]
fn recorded_lookup_and_pipeline_receipts_reject_subject_drift() {
    let fixture = recorded();
    let i = install();
    for (op, key) in [(Operation::Issue, "issue"), (Operation::MergeRequest, "mr")] {
        let input = values(json!({key:1}));
        let result = project_response(&i, op, &input, &fixture[key], "effect").unwrap();
        assert_eq!(result.0, "complete");
        assert_eq!(result.1[key], 1);
        let mut wrong = fixture[key].clone();
        wrong["project_id"] = json!(2);
        assert!(project_response(&i, op, &input, &wrong, "effect").is_err());
    }
    let pipeline = &fixture["pipelines"][0];
    let mut input =
        values(json!({"mr":1,"pipeline":1,"head":pipeline["sha"],"base":"a".repeat(40)}));
    assert_eq!(
        project_response(&i, Operation::Pipeline, &input, pipeline, "effect")
            .unwrap()
            .0,
        "failed"
    );
    input.insert("head".into(), json!("b".repeat(40)));
    assert_eq!(
        project_response(&i, Operation::Pipeline, &input, pipeline, "effect")
            .unwrap()
            .0,
        "stale"
    );
}

// Catches: publishing a complete-review receipt for a different version or a
// truncated/collapsed file set, including silent omission of one file.
#[test]
fn documented_diff_version_requires_exact_complete_artifact() {
    let raw = documented()["diff_version"].clone();
    assert!(raw.is_object());
    let input = values(
        json!({"mr":105,"version":raw["id"],"head":raw["head_commit_sha"],"base":raw["base_commit_sha"],"start":raw["start_commit_sha"]}),
    );
    let i = install();
    assert_eq!(
        project_response(&i, Operation::DiffVersion, &input, &raw, "e")
            .unwrap()
            .0,
        "complete"
    );
    for mutation in [
        "head_commit_sha",
        "base_commit_sha",
        "start_commit_sha",
        "collapsed",
        "too_large",
        "missing",
    ] {
        let mut wrong = raw.clone();
        let expected = if mutation.ends_with("sha") {
            wrong[mutation] = json!("f".repeat(40));
            "stale"
        } else {
            if mutation == "missing" {
                wrong["diffs"].as_array_mut().unwrap().pop();
            } else {
                wrong["diffs"][0][mutation] = json!(true);
            }
            "incomplete"
        };
        assert_eq!(
            project_response(&i, Operation::DiffVersion, &input, &wrong, "e")
                .unwrap()
                .0,
            expected
        );
    }
}

// Catches: a body without the original effect/artifact binding, or a response
// echo for a different note being accepted as this review's receipt.
#[test]
fn review_request_pins_artifact_and_rejects_unrelated_note() {
    let i = install();
    let input =
        values(json!({"mr":1,"head":"a".repeat(40),"base":"b".repeat(40),"body":"Review finding"}));
    let (url, method, body) = i
        .request(Operation::ReviewNote, &input, "original-effect")
        .unwrap();
    assert_eq!(
        url,
        "https://gitlab.example.invalid:8443/api/v4/projects/1/merge_requests/1/notes"
    );
    assert_eq!(method, "POST");
    let body: Value = serde_json::from_str(&body.unwrap()).unwrap();
    let text = body["body"].as_str().unwrap();
    assert!(text.contains(&format!("Head: {}", "a".repeat(40))));
    assert!(text.contains("tuic-effect:original-effect"));
    let mut commands = input.clone();
    commands.insert("body".into(), json!("Review\n/merge"));
    assert!(
        i.request(Operation::ReviewNote, &commands, "original-effect")
            .unwrap_err()
            .contains("quick actions")
    );
    // The documented service note is deliberately unrelated to this request.
    assert!(
        project_response(
            &i,
            Operation::ReviewNote,
            &input,
            &documented()["note"],
            "original-effect"
        )
        .is_err()
    );
}

// Catches: crossing project boundaries, basing source generation on delivery identity,
// or manufacturing a merge-base that the webhook never supplied.
#[test]
fn documented_events_keep_original_delivery_generation_and_unknown_base() {
    let docs = documented();
    // Official issue example contradicts project.id=1 with attributes.project_id=14.
    // Preserve the example unchanged and refuse it, rather than inventing evidence.
    assert!(event::normalize(&install(), "issue-delivery", &docs["issue_event"]).is_err());
    for name in ["merge_request_event", "pipeline_event"] {
        let raw = &docs[name];
        let mut i = install();
        i.project = raw["project"]["id"].as_u64().unwrap();
        let first = event::normalize(&i, "original-delivery", raw).unwrap();
        assert_eq!(
            event::normalize(&i, "original-delivery", raw).unwrap(),
            first
        );
        assert_eq!(
            event::normalize(&i, "redelivered-with-different-id", raw)
                .unwrap()
                .generation,
            first.generation
        );
        assert_eq!(first.base, None);
        let admitted = first.trigger().unwrap();
        assert_eq!(
            admitted.source,
            crate::workflows::trigger::TriggerSource::Gitlab
        );
        assert_eq!(admitted.delivery_id.as_deref(), Some("original-delivery"));
        assert_eq!(admitted.generation, first.generation);
        assert_eq!(admitted.values["project"], i.project);
        assert!(!admitted.values.contains_key("base"));
        // Incoming JSON cannot widen the installed source schema.
        let mut forged = admitted;
        forged.schema.fields.get_mut("project").unwrap().required = false;
        assert!(forged.validate().is_err());
        i.project += 1;
        assert!(event::normalize(&i, "original-delivery", raw).is_err());
    }
}
