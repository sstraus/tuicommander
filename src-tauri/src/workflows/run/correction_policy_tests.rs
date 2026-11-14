//! End-to-end retained role evidence plus native review/check floors.
use super::super::{native_test_support::Fixture, neutral_execution::*};
use crate::workflows::review_verdict::CodeVerdict;
use crate::workflows::{
    catalog::*, data_binding::DataSource, neutral_control::ControlNode,
    review_policy::ReviewAdmissionPolicy,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Catches source-role findings being dropped, accepted risk bypassing native
// checks, an invented caller report replacing the original and duplicate check spending.
#[tokio::test]
async fn policy_assessment_consumes_retained_role_findings_and_real_native_floors() {
    for (risk, expected) in [(5, true), (1, false)] {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().into());
        let mut f = Fixture::new();
        f.context = super::super::NativeContext::capture(
            &f.state,
            super::super::NativeCaller::Owner,
            f.context.workspace_path(),
            vec!["exact-tree".into()],
        )
        .unwrap();
        *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
        let (protocol, runtime) = super::super::neutral_model_adapter_tests::fixture("{}", false);
        {
            let mut config = f.state.config.write();
            config.ego_executable = runtime.executable.to_string_lossy().into_owned();
            config.ego_profile = runtime.profile.clone();
        }
        // Protocol files substitute only the external ACP boundary and are not
        // part of the reviewed artifact (avoids a self-referential commit digest).
        let names = std::fs::read_dir(protocol.path())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                std::fs::copy(&path, f.context.workspace_path().join(&name)).unwrap();
                name
            })
            .collect::<Vec<_>>();
        std::fs::write(
            f.context.workspace_path().join(".git/info/exclude"),
            names.join("\n"),
        )
        .unwrap();
        let mut policy = ReviewAdmissionPolicy {
            scope: "review-and-findings".into(),
            evidence: Default::default(),
        };
        policy.evidence.max_reviews = 4;
        let mut evidence = Value::Null;
        let run = super::super::correction_native_fixture::start_custom(&f, |d| {
            let catalog = crate::workflows::neutral_host::NeutralCatalogHost::catalog(
                f.host.as_ref(),
                &f.context.workspace_path().to_string_lossy(),
            )
            .unwrap();
            let state = match &d.input_bindings[&1]["state"] {
                DataSource::Constant { value } => value.clone(),
                _ => unreachable!(),
            };
            let a = &state["artifact"];
            evidence = json!({"subject":{"type":"story","id":"source-story","commit":a["head"],"tree":a["material_digest"],"requirements":a["criteria_digest"]},
                "findings":[{"id":"F1","risk":risk,"value":1,"effort":1,"addsMechanism":false,"deletesOnly":false,"scope":[],"acceptedRisk":"Named minor risk","proposal":null}],
                "resolvedFixes":[],"proof":{"realityEffect":"current candidate","realityState":"owned ledger","realityAssumptions":"external answer fixture","realityRadius":"native review/check","symptom":null,"red":null,"testChanges":[],"checks":[],"omissions":[]}});
            d.instances[0].descriptor = catalog
                .descriptors()
                .find(|v| v.handler.name == super::super::native_review_policy::HANDLER)
                .unwrap()
                .id
                .clone();
            d.instances[0].config.insert("policy".into(), json!(policy));
            d.control
                .edges
                .get_mut("work")
                .unwrap()
                .insert("exhausted".into(), "assess".into());
            d.instances[1].descriptor = catalog
                .descriptors()
                .find(|v| v.handler.name == super::super::correction_policy::HANDLER)
                .unwrap()
                .id
                .clone();
            d.instances[1].config.insert("policy".into(), json!(policy));
            d.input_bindings.get_mut(&1).unwrap().insert(
                "context".into(),
                DataSource::Constant {
                    value: json!({"origin":"source-story"}),
                },
            );
            d.input_bindings.get_mut(&1).unwrap().insert(
                "report".into(),
                DataSource::ActivityOutput {
                    activity: 2,
                    field: "report".into(),
                },
            );
            let mut request = super::super::neutral_model_adapter_tests::request();
            request.output_schema = ObjectSchema {
                fields: BTreeMap::from([(
                    "evidence".into(),
                    FieldSchema {
                        value_type: ValueType::Object,
                        required: true,
                        default: None,
                    },
                )]),
            };
            request.limits.max_output_bytes = 16_384;
            d.instances.push(ActivityInstance {
                descriptor: super::super::native_agent::descriptor(&runtime).unwrap().id,
                execution_policy: "local".into(),
                config: BTreeMap::from([
                    ("role".into(), json!("reviewer")),
                    ("policy".into(), json!(policy)),
                    ("request".into(), json!(request)),
                ]),
            });
            d.input_bindings.insert(2,BTreeMap::from([("source".into(),DataSource::Constant{value:json!({"origin":"source-story","generation":"1","commit":a["head"],"tree":a["material_digest"]})})]));
            d.control
                .nodes
                .insert("role".into(), ControlNode::Activity { activity: 2 });
            d.control
                .edges
                .get_mut("start")
                .unwrap()
                .insert("next".into(), "role".into());
            d.control
                .edges
                .insert("role".into(), [("complete".into(), "work".into())].into());
        });
        let scenario = f.context.workspace_path().join("scenario.jsonl");
        let steps = std::fs::read_to_string(&scenario)
            .unwrap()
            .lines()
            .map(|line| {
                let mut value: Value = serde_json::from_str(line).unwrap();
                if value["method"] == "session/update" {
                    value["params"]["update"]["content"]["text"] =
                        json!(json!({"evidence":evidence}).to_string());
                }
                value.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(scenario, steps).unwrap();
        let run = f.dispatch(&run).await;
        let at = super::super::store::now_ms();
        let run = super::super::neutral_execution_tests::step(&f.owner, &run, "role-done", at);
        let run = super::super::neutral_execution_tests::step(
            &f.owner,
            &run,
            "bounded-review-ready-after-role",
            at,
        );
        let (activity, _) = run.pending_invocations()[0];
        assert_eq!(
            run.descriptor(activity).unwrap().handler.name,
            super::super::native_review_policy::HANDLER,
            "a reused command id must not return the earlier role activation"
        );
        let run =
            super::super::correction_native_fixture::reviewed(&f, run, CodeVerdict::Pass).await;
        let after = f.dispatch(&run).await;
        let Some(NeutralResult::Completed { outputs, .. }) =
            &after.attempt.as_ref().unwrap().result
        else {
            panic!("{after:?}")
        };
        assert_eq!(outputs["repeat"], expected);
        // Decision 2(i): only an approving verdict of this run for exactly this story and
        // committed tree covers its transition; other stories and a fix verdict still ask.
        {
            use super::super::neutral_runtime::NeutralHost;
            let covered = |story: &str| {
                super::super::native_story::verdict_covers(
                    &f.context,
                    &f.owner.store,
                    &run.run.id,
                    story,
                )
            };
            assert_eq!(covered("source-story"), !expected);
            assert!(!covered("another-story"));
            let asks = |story: &str| {
                let transition = f.start(
                    "story.transition",
                    json!({"story":story,"revision":1,"command":"submit_review"}),
                );
                let (activity, attempt) =
                    transition.pending_invocations().into_iter().next().unwrap();
                let mut invocation = super::super::neutral_runtime::NeutralInvocation {
                    fence: attempt.fence.clone(),
                    publication: transition.publication.clone(),
                    descriptor: transition.descriptor(activity).unwrap().clone(),
                    instance: transition.publication.definition.instances[activity].clone(),
                    inputs: attempt.inputs.clone(),
                    subject: attempt.subject.clone(),
                };
                invocation.fence.run = run.run.id.clone();
                f.host.prepare(&invocation).unwrap().approval.is_some()
            };
            assert_eq!(asks("source-story"), expected);
            assert!(asks("another-story"));
        }
        assert_eq!(
            outputs["accounting"]["reviews"], 2,
            "role and native review each spend one admission"
        );
        assert_eq!(outputs["accounting"]["check_attempts"], 1);
        assert_eq!(outputs["state"]["findings"].as_array().unwrap().len(), 1);
        let assessment = after
            .effects
            .values()
            .find(|e| {
                e.invocation.descriptor.handler.name == super::super::correction_policy::HANDLER
            })
            .unwrap();
        let state = &outputs["state"]["artifact"];
        let cached = super::super::native_policy_checks::verify(
            &f.context,
            &f.owner.store,
            &assessment.key,
            "exact-tree",
            state["head"].as_str().unwrap(),
            state["material_digest"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(cached.exit_code, 0);
        let reopened = super::super::RunStore::open_at(f.owner.store.database_path()).unwrap();
        assert_eq!(
            super::super::native_policy_checks::count(&reopened, &run.run.id, &policy.scope)
                .unwrap(),
            1,
            "refresh and SQLite reopen must reuse the original check receipt"
        );
        // Read-back uses the retained report; a tampered source reference fails.
        let role = after
            .effects
            .values()
            .find(|e| e.invocation.descriptor.handler.name == super::super::native_agent::HANDLER)
            .unwrap();
        let bound = f.host.binding(&role.invocation).unwrap();
        let mut wrong_policy = policy.clone();
        wrong_policy.evidence.max_reviews = 5;
        let correction_state = serde_json::from_value(outputs["state"].clone()).unwrap();
        assert!(
            super::super::correction_policy::findings(
                &bound,
                assessment,
                &correction_state,
                &wrong_policy
            )
            .unwrap_err()
            .contains("policy differs"),
            "direct assessment cannot import a review from another budget"
        );
        let mut source = super::super::native_agent::source(&role.invocation).unwrap();
        source.generation = "2".into();
        assert!(super::super::native_agent::read_report(&bound, &role.key, &source).is_err());
        assert_eq!(
            super::super::native_review_policy::accounting(&f.owner.store, &after, &policy)
                .unwrap()["check_attempts"],
            1
        );
        // Simulate the persisted admission without its response. This may
        // reconcile the original attempt, but must not run another process.
        rusqlite::Connection::open(reopened.database_path())
            .unwrap()
            .execute(
                "UPDATE native_policy_checks SET receipt=NULL WHERE run=?1",
                [&run.run.id],
            )
            .unwrap();
        assert!(matches!(
            super::super::native_policy_checks::verify(
                &f.context,
                &reopened,
                &assessment.key,
                "exact-tree",
                state["head"].as_str().unwrap(),
                state["material_digest"].as_str().unwrap()
            ),
            Err(super::super::native_git::NativeFailure::Uncertain(_))
        ));
        assert_eq!(
            super::super::native_policy_checks::count(&reopened, &run.run.id, &policy.scope)
                .unwrap(),
            1
        );
    }
}
