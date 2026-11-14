use super::super::native_agent_test_support::{invocation, role_run};
use super::super::native_test_support::Fixture;
use super::super::neutral_effects::{EffectCommand, NeutralEffect};
use super::super::neutral_runtime::NeutralHost;
use serde_json::json;
use std::collections::BTreeMap;

// Catches role translation silently launching a broad profile or returning a
// model-supplied ownership/artifact claim instead of a source-bound native report.
#[tokio::test]
async fn native_agent_roles_execute_exact_mediated_profile_and_retain_source_bound_reports() {
    for role in ["implementer", "reviewer", "validator", "coordinator"] {
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
        tuic_git::git::git_commit_blocking(
            f.context.workspace_path().to_string_lossy().into_owned(),
            "protocol fixture".into(),
            Some(false),
        )
        .unwrap();
        let run = role_run(&f, role, &runtime);
        let source = run.inputs["source"].clone();
        let run = f.dispatch(&run).await;
        let effect = run.effects.values().next().unwrap();
        let super::super::neutral_effects::EffectOutcome::Completed {
            result: super::super::neutral_execution::NeutralResult::Completed { outputs, .. },
        } = &effect
            .observation
            .as_ref()
            .expect("model observation")
            .outcome
        else {
            panic!("{effect:?}")
        };
        let receipt = &outputs["report"];
        assert_eq!(receipt["source"], source);
        assert_eq!(receipt["effect"], json!(effect.key));
        assert_eq!(receipt["role"], role);
        assert_eq!(receipt["report"], json!({"next":7}));
        assert_eq!(receipt["identity"], json!(effect.model_session));
        assert!(
            effect
                .preparation
                .model_tools
                .as_ref()
                .unwrap()
                .tools
                .is_empty()
        );
        assert!(effect.settled());
        let bound = f.host.binding(&effect.invocation).unwrap();
        let expected: super::super::native_agent::Source =
            serde_json::from_value(source.clone()).unwrap();
        assert_eq!(
            super::super::native_agent::read_report(&bound, &effect.key, &expected).unwrap(),
            *receipt
        );
        let mut wrong_source = expected;
        wrong_source.generation = "foreign-generation".into();
        assert!(
            super::super::native_agent::read_report(&bound, &effect.key, &wrong_source)
                .unwrap_err()
                .contains("source/owner")
        );
        // An exact original callback cannot accept a foreign source or a late report.
        let mut foreign = effect.clone();
        foreign.invocation.inputs.get_mut("source").unwrap()["generation"] = json!("foreign");
        let bound = f.host.binding(&effect.invocation).unwrap();
        assert!(
            super::super::native_agent::report(
                &bound,
                &foreign,
                BTreeMap::from([("next".into(), json!(7))])
            )
            .is_err()
        );
        assert!(
            super::super::native_agent::report(
                &bound,
                effect,
                BTreeMap::from([("next".into(), json!(7))])
            )
            .is_err()
        );
    }
}

// Catches altered profile/prompt bypassing the pinned descriptor, a role label
// widening review tools, malformed reports, and source substitution at admission.
#[tokio::test]
async fn native_agent_contract_refuses_profile_drift_tools_and_stale_reports() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
    let runtime = crate::acp::EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "exact-profile".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    let run = role_run(&f, "reviewer", &runtime);
    let original = invocation(&run);
    let mut changed = original.clone();
    changed.instance.config.insert(
        "runtime".into(),
        json!({"executable":runtime.executable,"profile":"other"}),
    );
    assert!(
        super::super::native_model::prepare(&changed, &runtime)
            .unwrap_err()
            .contains("override")
    );
    let mut changed = original.clone();
    changed.instance.config.insert("role".into(), json!("root"));
    assert!(super::super::native_model::prepare(&changed, &runtime).is_err());
    let mut changed = original.clone();
    changed.inputs.get_mut("source").unwrap()["tree"] = json!("stale");
    let context = f.host.binding(&original).unwrap().context;
    let registry = f.host.registry();
    assert!(
        registry
            .admit_resolved(
                &context,
                &changed.publication,
                &changed.descriptor,
                &changed.instance,
                &changed.inputs,
                changed.subject.as_ref()
            )
            .is_err()
    );
    let mut tools = original.clone();
    let native = crate::workflows::native_operation_catalog().unwrap();
    let tool = native
        .descriptors()
        .find(|d| d.handler.name == "native.workspace.pin")
        .unwrap();
    tools.instance.config.insert(
        "tools".into(),
        json!(super::super::neutral_model_tools::ModelToolPolicy {
            tools: BTreeMap::from([(
                "workspace".into(),
                super::super::neutral_model_tools::ModelToolBinding {
                    descriptor: tool.clone(),
                    instance: crate::workflows::catalog::ActivityInstance {
                        descriptor: tool.id.clone(),
                        config: BTreeMap::new(),
                        execution_policy: "native".into()
                    }
                }
            )]),
            max_calls: 1,
            max_concurrency: 1,
        }),
    );
    assert!(
        super::super::native_model::prepare(&tools, &runtime)
            .unwrap_err()
            .contains("tool-less")
    );
    let request = super::super::native_agent::request(&original).unwrap();
    assert!(request.output("{\"next\":\"wrong\"}").is_err());
    assert!(request.output("{\"next\":7,\"owner\":\"forged\"}").is_err());
    let source_prompt: serde_json::Value = serde_json::from_str(&request.instructions).unwrap();
    assert_eq!(source_prompt["role"], "reviewer");
    assert_eq!(source_prompt["instructions"], "Transform literal context");
    let mut changed = original.clone();
    changed.instance.config.get_mut("request").unwrap()["instructions"] = json!("edited prompt");
    let a = NeutralEffect::prepare(original.clone(), f.host.prepare(&original).unwrap()).unwrap();
    let b = NeutralEffect::prepare(changed.clone(), f.host.prepare(&changed).unwrap()).unwrap();
    assert_ne!(a.key, b.key);
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(a.clone()),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: a.key.clone() })
        .unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::BindModel {
                key: a.key.clone(),
                session: super::super::neutral_model_tools::ModelSession {
                    connection: "connection".into(),
                    session: "original-reviewer".into(),
                    generation: 1,
                },
            },
        )
        .unwrap();
    assert!(
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::BindModel {
                    key: a.key,
                    session: super::super::neutral_model_tools::ModelSession {
                        connection: "connection".into(),
                        session: "replacement".into(),
                        generation: 2
                    }
                }
            )
            .is_err()
    );
    assert!(matches!(
        super::super::neutral_agent_adapter::restricted_agent().outcome,
        super::super::neutral_effects::EffectOutcome::Completed {
            result: super::super::neutral_execution::NeutralResult::Failed { .. }
        }
    ));
}

// Catches a role descriptor inheriting the host.model `text` input that the role
// request never sends, so a mapped text would be accepted and silently dropped.
#[test]
fn native_agent_role_refuses_the_model_text_input_it_never_uses() {
    let (_protocol, runtime) = super::super::neutral_model_adapter_tests::fixture("{}", false);
    let model = super::super::native_model::descriptor(&runtime).unwrap();
    assert!(model.inputs.fields.contains_key("text"));
    let role = super::super::native_agent::descriptor(&runtime).unwrap();
    let inputs = BTreeMap::from([
        (
            "source".to_string(),
            json!({"origin":"o","generation":"g","commit":"c","tree":"t"}),
        ),
        ("text".to_string(), json!("dropped")),
    ]);
    let refused = role.inputs.resolve(&inputs, "role inputs").unwrap_err();
    assert!(refused.contains("text: unknown field"), "{refused}");
}
