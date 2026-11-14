use super::super::neutral_model_tools::{
    ModelSession, ModelToolBinding, ModelToolCall, ModelToolPolicy,
};
use super::*;

fn parent(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    at: i64,
) -> (NeutralExecution, ModelToolCall) {
    let run = prepared(owner, run, at);
    let invocation = NeutralInvocation {
        fence: run.attempt.as_ref().unwrap().fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(0).unwrap().clone(),
        instance: run.publication.definition.instances[0].clone(),
        inputs: run.attempt.as_ref().unwrap().inputs.clone(),
        subject: run.subject.clone(),
    };
    let policy = ModelToolPolicy {
        max_calls: 1,
        max_concurrency: 1,
        tools: BTreeMap::from([(
            "increment".into(),
            ModelToolBinding {
                descriptor: invocation.descriptor.clone(),
                instance: invocation.instance.clone(),
            },
        )]),
    };
    let effect = NeutralEffect::prepare(
        invocation,
        EffectPreparation {
            mutation: false,
            resources: BTreeSet::new(),
            approval: None,
            model_tools: Some(policy),
        },
    )
    .unwrap();
    let key = effect.key.clone();
    let run = effect_command_at(
        owner,
        &run,
        "model-intent",
        EffectCommand::Prepare {
            effect: Box::new(effect),
        },
        at,
    )
    .unwrap();
    let run = effect_command_at(
        owner,
        &run,
        "model-dispatch",
        EffectCommand::Dispatch { key: key.clone() },
        at,
    )
    .unwrap();
    let session = ModelSession {
        connection: "01932d5e-0000-7000-8000-000000000001".into(),
        session: "original-session".into(),
        generation: 1,
    };
    let run = effect_command_at(
        owner,
        &run,
        "model-bind",
        EffectCommand::BindModel {
            key: key.clone(),
            session: session.clone(),
        },
        at,
    )
    .unwrap();
    (
        run,
        ModelToolCall {
            parent: key,
            session,
            id: "original-provider-call".into(),
            name: "increment".into(),
        },
    )
}

fn parent_result(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    call: &ModelToolCall,
    at: i64,
) -> NeutralExecution {
    effect_command_at(
        owner,
        run,
        "parent-result",
        EffectCommand::Observe {
            key: call.parent.clone(),
            observation: EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Completed {
                    result: NeutralResult::Completed {
                        outcome: "ok".into(),
                        outputs: BTreeMap::from([("next".into(), json!(99))]),
                    },
                },
            },
        },
        at,
    )
    .unwrap()
}

// Catches duplicate RPC writes, early parent completion, and result loss at reopen.
// This is kernel/owned-file evidence, not ACP provider-turn continuation proof.
#[tokio::test]
async fn model_tool_intent_precedes_write_and_parent_waits_for_retained_result() {
    let (dir, owner, run, at) = fixture(30, false);
    let (run, call) = parent(&owner, &run, at);
    let host = FileHost::new(
        &owner,
        dir.path().join("model-write"),
        Reply::Complete,
        false,
    );
    owner.neutral_host(host.clone());
    let args = BTreeMap::from([("value".into(), json!(30))]);
    let effect = owner.start_model_tool(call.clone(), args.clone()).unwrap();
    assert!(effect.dispatched);
    assert_eq!(claims(&owner), 1);
    let duplicate = owner.start_model_tool(call.clone(), args.clone()).unwrap();
    assert_eq!(duplicate.key, effect.key);
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    let run = owner.store.neutral_execution(&run.run.id).unwrap();
    let run = parent_result(&owner, &run, &call, at);
    assert!(run.attempt.as_ref().unwrap().result.is_none());
    tokio::task::yield_now().await;
    let run = owner.store.neutral_execution(&run.run.id).unwrap();
    assert!(run.attempt.as_ref().unwrap().result.is_some());
    assert_eq!(claims(&owner), 0);
    assert!(run.model_tool_call(&call).unwrap().unwrap().settled());
    let db = owner.store.database_path().to_path_buf();
    drop(host);
    drop(owner);
    let reopened = RuntimeOwner::acquire(&db).unwrap();
    let retained = reopened.start_model_tool(call, args).unwrap();
    assert_eq!(retained.key, effect.key);
    assert!(retained.settled());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("model-write"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

// Catches replacing an uncertain original call with a new write after host restart.
#[tokio::test]
async fn model_tool_lost_write_reconciles_same_call_without_reinvoking() {
    let (dir, owner, run, at) = fixture(30, false);
    let (run, call) = parent(&owner, &run, at);
    let file = dir.path().join("lost-model-write");
    let host = FileHost::new(&owner, file.clone(), Reply::Lost, false);
    owner.neutral_host(host.clone());
    let args = BTreeMap::from([("value".into(), json!(30))]);
    let effect = owner.start_model_tool(call.clone(), args.clone()).unwrap();
    let run = owner.store.neutral_execution(&run.run.id).unwrap();
    parent_result(&owner, &run, &call, at);
    tokio::task::yield_now().await;
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    let db = owner.store.database_path().to_path_buf();
    let id = run.run.id.clone();
    drop(host);
    drop(owner);
    let reopened = RuntimeOwner::acquire(&db).unwrap();
    let host = FileHost::new(&reopened, file.clone(), Reply::Lost, false);
    reopened.neutral_host(host.clone());
    let original = reopened
        .start_model_tool(call.clone(), args.clone())
        .unwrap();
    assert_eq!(original.key, effect.key);
    assert_eq!(host.calls.load(Ordering::Acquire), 0);
    super::super::runtime::run_actor(
        Arc::downgrade(&reopened),
        id.clone(),
        Arc::new(Notify::new()),
    )
    .await;
    let reconciled = reopened.start_model_tool(call, args).unwrap();
    assert!(reconciled.settled());
    assert_eq!(host.calls.load(Ordering::Acquire), 0);
    assert_eq!(host.reconciles.load(Ordering::Acquire), 1);
    assert_eq!(std::fs::read_to_string(file).unwrap().lines().count(), 1);
    assert!(matches!(
        reopened.store.neutral_execution(&id).unwrap().status,
        NeutralStatus::Completed { .. }
    ));
}

// Catches call-ID argument/name replacement, generation theft, and unbounded calls.
#[tokio::test]
async fn model_tool_identity_allowlist_and_limits_refuse_changed_requests() {
    let (dir, owner, run, at) = fixture(30, false);
    let (_, call) = parent(&owner, &run, at);
    let host = FileHost::new(
        &owner,
        dir.path().join("bounded-write"),
        Reply::Complete,
        false,
    );
    owner.neutral_host(host.clone());
    let args = BTreeMap::from([("value".into(), json!(30))]);
    owner.start_model_tool(call.clone(), args.clone()).unwrap();
    assert!(
        owner
            .start_model_tool(call.clone(), BTreeMap::from([("value".into(), json!(31))]))
            .is_err()
    );
    let mut changed = call.clone();
    changed.name = "shell".into();
    assert!(owner.start_model_tool(changed, args.clone()).is_err());
    let mut changed = call.clone();
    changed.session.generation += 1;
    assert!(owner.start_model_tool(changed, args.clone()).is_err());
    let mut changed = call;
    changed.id = "second-call".into();
    assert!(owner.start_model_tool(changed, args).is_err());
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    tokio::task::yield_now().await;
}

// Catches two concurrent duplicate RPCs both treating an idempotent dispatch
// receipt as permission to enter the writer.
#[tokio::test]
async fn concurrent_model_tool_duplicates_enter_the_writer_once() {
    let (dir, owner, run, at) = fixture(30, false);
    let (_, call) = parent(&owner, &run, at);
    let host = FileHost::new(
        &owner,
        dir.path().join("concurrent-write"),
        Reply::Complete,
        false,
    );
    owner.neutral_host(host.clone());
    let barrier = std::sync::Barrier::new(2);
    let handle = tokio::runtime::Handle::current();
    let keys = std::thread::scope(|scope| {
        let invoke = || {
            let _entered = handle.enter();
            barrier.wait();
            owner
                .start_model_tool(call.clone(), BTreeMap::from([("value".into(), json!(30))]))
                .unwrap()
                .key
        };
        let first = scope.spawn(invoke);
        let second = scope.spawn(invoke);
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(keys.0, keys.1);
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    tokio::task::yield_now().await;
    assert_eq!(
        std::fs::read_to_string(dir.path().join("concurrent-write"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[path = "neutral_model_mcp_tests.rs"]
mod mcp;

#[path = "neutral_model_recovery_tests.rs"]
mod recovery;
