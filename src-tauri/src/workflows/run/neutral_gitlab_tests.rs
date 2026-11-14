use super::neutral_effects::EffectOutcome;
use super::neutral_execution::{NeutralResult, NeutralStatus};
use super::neutral_gitlab::GitLabContext;
use super::neutral_gitlab_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::{gitlab::Operation, neutral_admission::HostAdmission};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, atomic::Ordering};
fn recorded() -> Value {
    serde_json::from_str(include_str!(
        "../../../../plugins/gitlab/fixtures/recorded.json"
    ))
    .unwrap()
}
fn input(raw: &Value) -> BTreeMap<String, Value> {
    serde_json::from_value(json!({"mr":1,"head":raw["sha"],"base":raw["diff_refs"]["base_sha"],"body":"Review result"})).unwrap()
}

// Catches: graph-supplied installed metadata or an old publication grant
// authorizing a note after current account/project rights have been revoked.
#[test]
fn installed_admission_rejects_forged_metadata_and_revoked_grants() {
    let install = crate::workflows::gitlab::GitLabInstall::new(
        "https://gitlab.example.invalid/api/v4/".into(),
        1,
        "account".into(),
    )
    .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let f = fixture(
        state,
        install,
        Operation::ReviewNote,
        input(&recorded()["mr"]),
    );
    let i = &f.invocation;
    let read_only = GitLabContext {
        reads: BTreeSet::from([f.host.install.connector.id.clone()]),
        writes: BTreeSet::new(),
    };
    assert!(
        f.host
            .publication(&read_only, &i.publication, &i.descriptor, &i.instance)
            .unwrap_err()
            .contains("grant")
    );
    let mut instance = i.instance.clone();
    instance
        .config
        .insert("installed".into(), json!({"project":2}));
    let context = GitLabContext {
        reads: read_only.reads.clone(),
        writes: read_only.reads,
    };
    assert!(
        f.host
            .publication(&context, &i.publication, &i.descriptor, &instance)
            .unwrap_err()
            .contains("override")
    );
    f.granted.store(false, Ordering::Release);
    assert!(f.host.prepare(i).unwrap_err().contains("grant"));
}

// Catches: bypassing the existing authenticated account client, losing artifact
// identity from the retained receipt, or replay dispatching the note twice.
#[tokio::test]
async fn general_note_runs_through_owned_effect_and_is_not_duplicated() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let raw = recorded()["mr"].clone();
    let (state, install, seen, task) = server(dir.path(), raw.clone(), 201).await;
    let f = fixture(state, install, Operation::ReviewNote, input(&raw));
    let first = run(&f).await;
    assert_eq!(
        first.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    let receipt = &first.outputs.as_ref().unwrap().values["receipt"];
    assert_eq!(receipt["head"], raw["sha"]);
    assert_eq!(receipt["base"], raw["diff_refs"]["base_sha"]);
    assert_eq!(receipt["note"], 99);
    assert_eq!(run(&f).await.sequence, first.sequence);
    let requests = seen.lock().unwrap();
    let writes: Vec<_> = requests.iter().filter(|r| r.0 == "POST").collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].1, "/api/v4/projects/1/merge_requests/1/notes");
    assert!(
        writes[0].2["body"]
            .as_str()
            .unwrap()
            .contains(receipt["effect"].as_str().unwrap())
    );
    drop(requests);
    task.abort();
}

// Catches: HTTP 500 after a possible commit being interpreted as safe to repeat
// the write; reconciliation must retain ownership and make no new request.
#[tokio::test]
async fn uncertain_note_reconciliation_never_reposts() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let raw = recorded()["mr"].clone();
    let (state, install, seen, task) = server(dir.path(), raw.clone(), 500).await;
    let f = fixture(state, install, Operation::ReviewNote, input(&raw));
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
    // Observe the real durable uncertainty event, rather than waiting for the
    // unrelated workflow deadline to make the actor return.
    let effect = loop {
        let changed = f.owner.neutral_effect_updates.notified();
        let snapshot = f
            .owner
            .store
            .neutral_execution(&f.invocation.fence.run)
            .unwrap();
        if let Some(effect) = snapshot.effects.values().find(|effect| {
            matches!(&effect.observation, Some(o) if matches!(o.outcome, EffectOutcome::Uncertain { .. }))
        }) { break effect.clone(); }
        changed.await;
    };
    actor.abort();
    assert!(!effect.settled());
    let before = seen.lock().unwrap().len();
    let observation = f.host.reconcile(effect.clone()).await.unwrap();
    assert!(matches!(
        observation.outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert!(!observation.stopped);
    assert_eq!(seen.lock().unwrap().len(), before);
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "POST")
            .count(),
        1
    );
    task.abort();
}

// Catches: a preflight head mismatch still permitting publication, even when
// the configured account and a write grant are otherwise valid.
#[tokio::test]
async fn stale_head_returns_stale_without_a_post() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let raw = recorded()["mr"].clone();
    let (state, install, seen, task) = server(dir.path(), raw.clone(), 201).await;
    let mut values = input(&raw);
    values.insert("head".into(), json!("a".repeat(40)));
    let f = fixture(state, install, Operation::ReviewNote, values);
    let result = run(&f).await;
    assert!(result.effects.values().any(|e|matches!(&e.observation,Some(o) if matches!(&o.outcome,EffectOutcome::Completed {result:NeutralResult::Completed {outcome,..}} if outcome=="stale"))));
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "POST")
            .count(),
        0
    );
    task.abort();
}

// Catches: accepting a same-SHA pipeline from another MR as this MR's CI, or
// dropping typed head/base/pipeline outputs needed by downstream activities.
#[tokio::test]
async fn exact_mr_pipeline_receipt_keeps_failure_and_typed_artifact_fields() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let data = recorded();
    let raw = &data["mr"];
    let (state, install, seen, task) = server(
        dir.path(),
        json!({"mr":raw,"pipeline":data["pipelines"][0]}),
        201,
    )
    .await;
    let mut values = input(raw);
    values.remove("body");
    values.insert("pipeline".into(), data["pipelines"][0]["id"].clone());
    let f = fixture(
        state.clone(),
        install.clone(),
        Operation::Pipeline,
        values.clone(),
    );
    let result = run(&f).await;
    let observation = result
        .effects
        .values()
        .next()
        .unwrap()
        .observation
        .as_ref()
        .unwrap();
    match &observation.outcome {
        EffectOutcome::Completed {
            result: NeutralResult::Completed { outcome, outputs },
        } => {
            assert_eq!(outcome, "failed");
            assert_eq!(outputs["head"], raw["sha"]);
            assert_eq!(outputs["base"], raw["diff_refs"]["base_sha"]);
            assert_eq!(outputs["pipeline"], data["pipelines"][0]["id"]);
        }
        other => panic!("expected failed pipeline receipt: {other:?}"),
    }
    let before = seen.lock().unwrap().len();
    values.insert("pipeline".into(), json!(999));
    let other = fixture(state, install, Operation::Pipeline, values);
    let result = run(&other).await;
    assert!(result.effects.values().any(|e|matches!(&e.observation,Some(o) if matches!(&o.outcome,EffectOutcome::Completed {result:NeutralResult::Completed {outcome,..}} if outcome=="stale"))));
    assert_eq!(
        seen.lock().unwrap().len(),
        before + 1,
        "wrong pipeline must stop after the MR preflight"
    );
    task.abort();
}
