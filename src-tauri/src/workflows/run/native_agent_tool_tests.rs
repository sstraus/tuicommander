use super::super::native_agent_test_support::{invocation, role_run_with};
use super::super::native_test_support::Fixture;
use super::super::neutral_effects::{EffectCommand, NeutralEffect};
use super::super::neutral_model_tools::{
    ModelSession, ModelToolBinding, ModelToolCall, ModelToolPolicy,
};
use super::super::neutral_runtime::NeutralHost;
use crate::agent_capabilities::CapabilitySet;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};
use serde_json::json;
use std::collections::BTreeMap;

// Catches a scoped implementer bypassing exact native write approval, inheriting
// fresh rights on another call, or repeating its original story write on replay.
#[tokio::test]
async fn native_agent_implementer_uses_owned_native_story_tool_and_current_scope() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            project: f.context.workspace_path().to_string_lossy().into_owned(),
            title: "Scoped implementation".into(),
            source: "test source".into(),
        })
        .unwrap();
    let story = stories
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Scoped native write".into(),
            criteria: vec!["Native evidence".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let story = stories
        .transition(&story.id, story.revision, StoryCommand::StartManual)
        .unwrap();
    f.state.agent_capabilities.insert(
        "trusted".into(),
        CapabilitySet::from_spawn_arg(&json!(["operator"])).unwrap(),
    );
    let catalog = crate::workflows::native_operation_catalog().unwrap();
    let tool = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.story.transition")
        .unwrap();
    let policy = ModelToolPolicy {
        tools: BTreeMap::from([(
            "update_story".into(),
            ModelToolBinding {
                descriptor: tool.clone(),
                instance: crate::workflows::catalog::ActivityInstance {
                    descriptor: tool.id.clone(),
                    config: BTreeMap::new(),
                    execution_policy: "native".into(),
                },
            },
        )]),
        max_calls: 2,
        max_concurrency: 1,
    };
    let runtime = crate::acp::EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "exact-profile".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    let run = role_run_with(
        &f,
        "implementer",
        &runtime,
        super::super::NativeCaller::Agent("trusted".into()),
        Some(policy),
    );
    let invocation = invocation(&run);
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let key = effect.key.clone();
    for command in [
        EffectCommand::Prepare {
            effect: Box::new(effect.clone()),
        },
        EffectCommand::Dispatch { key: key.clone() },
        EffectCommand::BindModel {
            key: key.clone(),
            session: ModelSession {
                connection: "connection".into(),
                session: "scoped-implementer".into(),
                generation: 1,
            },
        },
    ] {
        f.owner
            .neutral_effect_command(&run.run.id, command)
            .unwrap();
    }
    let parent = f.owner.store.neutral_execution(&run.run.id).unwrap();
    let session = parent.effect(&key).unwrap().model_session.clone().unwrap();
    let call = ModelToolCall {
        parent: key.clone(),
        session,
        id: "original-write".into(),
        name: "update_story".into(),
    };
    let arguments = BTreeMap::from([(
        "request".into(),
        json!({"story":story.id,"revision":story.revision,"command":{"check_criterion":0}}),
    )]);
    assert!(
        f.owner
            .start_model_tool(call.clone(), arguments.clone())
            .is_err()
    );
    assert!(!stories.get_story(&story.id).unwrap().checked[0]);
    let parent = f.owner.store.neutral_execution(&run.run.id).unwrap();
    let child = parent.effects.values().find(|e| e.tool.is_some()).unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Answer {
                key: child.key.clone(),
                revision: 1,
                approved: true,
            },
        )
        .unwrap();
    let child = f
        .owner
        .start_model_tool(call.clone(), arguments.clone())
        .unwrap();
    loop {
        let changed = f.owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if f.owner
            .store
            .neutral_execution(&run.run.id)
            .unwrap()
            .effect(&child.key)
            .unwrap()
            .settled()
        {
            break;
        }
        changed.await;
    }
    let changed = stories.get_story(&story.id).unwrap();
    assert!(changed.checked[0]);
    let original = f
        .owner
        .start_model_tool(call.clone(), arguments.clone())
        .unwrap();
    assert_eq!(original.key, child.key);
    assert_eq!(
        stories.get_story(&story.id).unwrap().revision,
        changed.revision
    );
    f.state.agent_capabilities.remove("trusted");
    let mut next = call;
    next.id = "revoked-write".into();
    assert!(
        f.owner
            .start_model_tool(next, arguments)
            .unwrap_err()
            .contains("revoked")
    );
}
