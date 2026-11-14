use super::super::{native_test_support::Fixture, neutral_effects::*, neutral_execution::*};
use crate::workflows::{neutral_control::ControlNode, review_policy::ReviewAdmissionPolicy};
use serde_json::json;

// Catches a fresh replacement role opening another ACP session after the shared
// immutable budget is spent, including a retained refusal surviving SQLite reopen.
#[tokio::test]
async fn policy_role_spends_shared_budget_and_exhaustion_never_starts_replacement() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
    let (protocol, runtime) =
        super::super::neutral_model_adapter_tests::fixture("{\"next\":7}", false);
    for entry in std::fs::read_dir(protocol.path()).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(
            &path,
            f.context.workspace_path().join(path.file_name().unwrap()),
        )
        .unwrap();
    }
    super::super::check::git_output(f.context.workspace_path(), &["add", "."]).unwrap();
    super::super::check::git_output(
        f.context.workspace_path(),
        &["commit", "-qm", "protocol fixture"],
    )
    .unwrap();
    let mut policy = ReviewAdmissionPolicy {
        scope: "shared-review".into(),
        evidence: Default::default(),
    };
    policy.evidence.max_reviews = 1;
    policy.evidence.max_repairs = 0;
    let run = super::super::native_agent_test_support::role_run_custom(
        &f,
        "reviewer",
        &runtime,
        super::super::NativeCaller::Owner,
        None,
        |d| {
            d.instances[0].config.insert("policy".into(), json!(policy));
            d.instances.push(d.instances[0].clone());
            d.input_bindings.insert(1, d.input_bindings[&0].clone());
            d.control
                .nodes
                .insert("replacement".into(), ControlNode::Activity { activity: 1 });
            d.control
                .edges
                .get_mut("work")
                .unwrap()
                .insert("complete".into(), "replacement".into());
            d.control.edges.insert(
                "replacement".into(),
                [("complete".into(), "done".into())].into(),
            );
        },
    );
    let run = f.dispatch(&run).await;
    let first = run.effects.values().next().unwrap();
    assert!(
        first.model_session.is_some(),
        "the first admission really entered the ACP fixture"
    );
    assert_eq!(
        super::super::native_review_policy::ordinal(&f.owner.store, &first.key, &policy).unwrap(),
        1
    );
    let at = super::super::store::now_ms();
    let run = super::super::neutral_execution_tests::step(&f.owner, &run, "next-role", at);
    let run = super::super::neutral_execution_tests::step(&f.owner, &run, "prepare-role", at);
    let run = f.dispatch(&run).await;
    let second = run.effects.values().find(|e| e.key != first.key).unwrap();
    assert!(
        second.model_session.is_none(),
        "exhaustion must happen before ACP connect"
    );
    let Some(EffectObservation {
        stopped: true,
        outcome:
            EffectOutcome::Completed {
                result: NeutralResult::Completed { outputs, .. },
            },
    }) = &second.observation
    else {
        panic!("{second:?}")
    };
    assert_eq!(outputs["report"]["admission_exhausted"], true);
    let reopened = super::super::RunStore::open_at(f.owner.store.database_path()).unwrap();
    assert_eq!(
        reopened
            .neutral_execution(&run.run.id)
            .unwrap()
            .effect(&second.key)
            .unwrap(),
        second
    );
    assert_eq!(
        super::super::native_review_policy::accounting(&reopened, &run, &policy).unwrap()["reviews"],
        1
    );
}
