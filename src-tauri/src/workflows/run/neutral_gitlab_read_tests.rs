use super::neutral_effects::EffectOutcome;
use super::neutral_execution::NeutralResult;
use super::neutral_gitlab_fixture::*;
use crate::workflows::gitlab::Operation;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

// Catches: a completed read GET with an invalid service reply retaining permanent
// uncertainty instead of settling Failed, despite having no external mutation.
#[tokio::test]
async fn malformed_read_reply_settles_failed_instead_of_remaining_uncertain() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let (state, install, seen, task) = server(dir.path(), Value::Null, 201).await;
    let f = fixture(
        state,
        install,
        Operation::Issue,
        BTreeMap::from([("issue".into(), json!(1))]),
    );
    let owner = f.owner.clone();
    let run_id = f.invocation.fence.run.clone();
    let actor = tokio::spawn(async move {
        super::runtime::run_actor(
            Arc::downgrade(&owner),
            run_id,
            Arc::new(tokio::sync::Notify::new()),
        )
        .await;
    });
    let effect = loop {
        let changed = f.owner.neutral_effect_updates.notified();
        let snapshot = f
            .owner
            .store
            .neutral_execution(&f.invocation.fence.run)
            .unwrap();
        if let Some(effect) = snapshot.effects.values().find(|e| e.observation.is_some()) {
            break effect.clone();
        }
        changed.await;
    };
    actor.abort();
    let _ = actor.await;
    task.abort();
    let observation = effect.observation.as_ref().unwrap();
    assert!(
        observation.stopped,
        "a completed non-mutating GET must release uncertainty: {observation:?}"
    );
    assert!(matches!(&observation.outcome,
        EffectOutcome::Completed { result: NeutralResult::Failed { diagnostic } }
        if diagnostic.contains("project_id")));
    assert!(effect.settled());
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.iter().filter(|r| r.1.ends_with("/issues/1")).count(),
        1
    );
    assert!(!seen.iter().any(|r| r.0 == "POST"));
}
