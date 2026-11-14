use super::neutral_effects::{
    EffectCommand, EffectKey, EffectObservation, EffectOutcome, EffectPreparation, EffectQuestion,
    NeutralEffect,
};
use super::neutral_execution::{
    NeutralExecution, NeutralExecutionCommand as Command, NeutralResult, NeutralStatus,
};
use super::neutral_execution_tests::{fixture, step};
use super::neutral_runtime::{NeutralHost, NeutralInvocation, ObservationFuture};
use super::neutral_runtime_tests::Host;
use super::runtime::RuntimeOwner;
use crate::workflows::neutral_admission::{HostContext, HostRegistry};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
enum Reply {
    Early,
    Lost,
    Pending,
    Complete,
}

/// An owned Rust adapter, not a simulation of a connector or OS process. Its
/// real write is a file under test_temp_root; reconciliation reads that file.
struct FileHost {
    admission: Arc<Host>,
    owner: Weak<RuntimeOwner>,
    file: PathBuf,
    calls: AtomicUsize,
    reconciles: AtomicUsize,
    stops: AtomicUsize,
    reply: Reply,
    approval: bool,
}
impl FileHost {
    fn new(owner: &Arc<RuntimeOwner>, file: PathBuf, reply: Reply, approval: bool) -> Arc<Self> {
        Arc::new(Self {
            admission: Host::new(false, false),
            owner: Arc::downgrade(owner),
            file,
            calls: AtomicUsize::new(0),
            reconciles: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            reply,
            approval,
        })
    }
    fn result(effect: &NeutralEffect) -> EffectObservation {
        EffectObservation {
            stopped: true,
            outcome: EffectOutcome::Completed {
                result: NeutralResult::Completed {
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([(
                        "next".into(),
                        json!(effect.invocation.inputs["value"].as_i64().unwrap() + 1),
                    )]),
                },
            },
        }
    }
}
impl NeutralHost for FileHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        self.admission.context(invocation)
    }
    fn registry(&self) -> HostRegistry<'_> {
        self.admission.registry()
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let diagnosis = invocation.inputs["value"] == json!(20);
        let deadline = self
            .owner
            .upgrade()
            .unwrap()
            .store
            .neutral_execution(&invocation.fence.run)?
            .deadline_ms;
        Ok(EffectPreparation {
            model_tools: None,
            mutation: !diagnosis,
            resources: if diagnosis {
                BTreeSet::new()
            } else {
                BTreeSet::from(["owned-test-file".into()])
            },
            approval: (self.approval && !diagnosis).then(|| EffectQuestion {
                revision: 1,
                title: "Write the exact bound value?".into(),
                deadline_ms: deadline,
            }),
        })
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        let owner = self.owner.upgrade().unwrap();
        let retained = owner
            .store
            .neutral_execution(&effect.key.fence.run)
            .unwrap();
        assert!(
            retained.effect(&effect.key).unwrap().dispatched,
            "intent and correlation must commit before adapter entry"
        );
        let count: i64 = owner
            .store
            .connect()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM neutral_effect_resources WHERE run_id=?1 AND effect_id=?2",
                rusqlite::params![effect.key.fence.run, effect.key.digest],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, if effect.preparation.mutation { 1 } else { 0 });
        self.calls.fetch_add(1, Ordering::AcqRel);
        if effect.preparation.mutation {
            std::fs::write(&self.file, format!("{}\n", effect.key.digest)).unwrap();
        }
        let result = Self::result(&effect);
        if matches!(self.reply, Reply::Early) {
            owner
                .neutral_effect_command(
                    &effect.key.fence.run,
                    EffectCommand::Observe {
                        key: effect.key.clone(),
                        observation: result.clone(),
                    },
                )
                .unwrap();
        }
        let reply = if effect.preparation.mutation {
            self.reply
        } else {
            Reply::Complete
        };
        Box::pin(async move {
            match reply {
                Reply::Lost => Err("response lost after the owned write".into()),
                Reply::Early | Reply::Pending => Ok(EffectObservation {
                    stopped: false,
                    outcome: EffectOutcome::Pending {
                        handle: "owned-operation-1".into(),
                    },
                }),
                Reply::Complete => Ok(result),
            }
        })
    }
    fn reconcile(&self, effect: NeutralEffect) -> ObservationFuture {
        self.reconciles.fetch_add(1, Ordering::AcqRel);
        let evidence = if matches!(self.reply, Reply::Pending) {
            Err(std::io::Error::other("owned job stop is not yet proven"))
        } else {
            std::fs::read_to_string(&self.file)
        };
        Box::pin(async move {
            if evidence.is_ok_and(|value| value == format!("{}\n", effect.key.digest)) {
                Ok(Self::result(&effect))
            } else {
                Err("original write cannot be proven".into())
            }
        })
    }
    fn cancel(&self, _: NeutralEffect) -> ObservationFuture {
        self.stops.fetch_add(1, Ordering::AcqRel);
        Box::pin(async {
            Ok(EffectObservation {
                stopped: false,
                outcome: EffectOutcome::Uncertain {
                    diagnostic: "stop requested; no stopped process evidence".into(),
                },
            })
        })
    }
}

fn prepared(owner: &RuntimeOwner, run: &NeutralExecution, at: i64) -> NeutralExecution {
    let run = step(owner, run, "enter", at);
    step(owner, &run, "prepare-activation", at)
}
fn effect_command_at(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    name: &str,
    command: EffectCommand,
    at: i64,
) -> Result<NeutralExecution, String> {
    owner.neutral_command_at(
        &run.run.id,
        name,
        run.sequence,
        Command::Effect { command },
        at,
    )
}
async fn dispatch(owner: &Arc<RuntimeOwner>, id: &str) -> NeutralExecution {
    let notify = Arc::new(Notify::new());
    let run = owner.store.neutral_execution(id).unwrap();
    assert!(owner.dispatch_neutral(&run, &notify).unwrap());
    notify.notified().await;
    owner.store.neutral_execution(id).unwrap()
}
fn only_key(run: &NeutralExecution) -> EffectKey {
    run.effects.values().next().unwrap().key.clone()
}
fn claims(owner: &RuntimeOwner) -> i64 {
    owner
        .store
        .connect()
        .unwrap()
        .query_row("SELECT count(*) FROM neutral_effect_resources", [], |r| {
            r.get(0)
        })
        .unwrap()
}

// Catches: a callback beating Pending being overwritten, a conflicting duplicate
// changing immutable output, or raw completion bypassing exact effect identity.
#[tokio::test]
async fn early_callback_wins_pending_ack_and_exact_results_are_immutable() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Early, false);
    owner.neutral_host(host.clone());
    let settled = dispatch(&owner, &run.run.id).await;
    let key = only_key(&settled);
    let effect = settled.effect(&key).unwrap();
    assert!(effect.settled());
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        std::fs::read_to_string(&host.file).unwrap(),
        format!("{}\n", key.digest)
    );
    assert_eq!(claims(&owner), 0);
    let duplicate = owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: FileHost::result(effect),
            },
        )
        .unwrap();
    assert_eq!(duplicate.sequence, settled.sequence);
    let mut wrong = FileHost::result(effect);
    if let EffectOutcome::Completed {
        result: NeutralResult::Completed { outputs, .. },
    } = &mut wrong.outcome
    {
        outputs.insert("next".into(), json!(999));
    }
    assert!(
        owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: wrong
                }
            )
            .unwrap_err()
            .contains("immutable")
    );
    assert!(
        owner
            .neutral_command(
                &run.run.id,
                "uncorrelated",
                settled.sequence,
                Command::Complete {
                    fence: key.fence.clone(),
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(7))])
                }
            )
            .unwrap_err()
            .contains("correlation")
    );
    let (finished, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    assert_eq!(finished.outputs.unwrap().values["result"], json!(7));
}

// Catches: restart reissuing a write whose response was lost, releasing its
// resource on a transport error, or reconstructing a different correlation.
#[tokio::test]
async fn lost_write_response_survives_restart_and_reconciles_without_second_write() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let path = dir.path().join("owned-write");
    let host = FileHost::new(&owner, path.clone(), Reply::Lost, false);
    owner.neutral_host(host.clone());
    let uncertain = dispatch(&owner, &run.run.id).await;
    let key = only_key(&uncertain);
    assert!(matches!(
        uncertain
            .effect(&key)
            .unwrap()
            .observation
            .as_ref()
            .unwrap()
            .outcome,
        EffectOutcome::Uncertain { .. }
    ));
    assert_eq!(uncertain.status, NeutralStatus::Running);
    assert!(uncertain.attempt.as_ref().unwrap().result.is_none());
    assert_eq!(claims(&owner), 1);
    drop(host);
    drop(owner);
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let host = FileHost::new(&owner, path, Reply::Lost, false);
    owner.neutral_host(host.clone());
    super::runtime::run_actor(
        Arc::downgrade(&owner),
        run.run.id.clone(),
        Arc::new(Notify::new()),
    )
    .await;
    let finished = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(finished.outputs.unwrap().values["result"], json!(7));
    assert_eq!(host.calls.load(Ordering::Acquire), 0);
    assert_eq!(host.reconciles.load(Ordering::Acquire), 1);
    assert!(finished.effects.contains_key(&key.digest));
    assert_eq!(claims(&owner), 0);
}

// Catches: an expired/stale approval granting execution, one answer overwriting
// a competing committed answer, or changed arguments/subject reusing the yes.
#[test]
fn exact_approval_revision_has_one_committed_winner_and_never_expires_to_yes() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let (_, attempt) = run.pending_invocations()[0];
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(0).unwrap().clone(),
        instance: run.publication.definition.instances[0].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    let effect = NeutralEffect::prepare(
        invocation,
        EffectPreparation {
            model_tools: None,
            mutation: true,
            resources: BTreeSet::from(["resource".into()]),
            approval: Some(EffectQuestion {
                revision: 1,
                title: "Exact write?".into(),
                deadline_ms: at + 1000,
            }),
        },
    )
    .unwrap();
    let key = effect.key.clone();
    let run = effect_command_at(
        &owner,
        &run,
        "intent",
        EffectCommand::Prepare {
            effect: Box::new(effect),
        },
        at,
    )
    .unwrap();
    let run = effect_command_at(
        &owner,
        &run,
        "revised",
        EffectCommand::Question {
            key: key.clone(),
            question: EffectQuestion {
                revision: 2,
                title: "Confirm exact write?".into(),
                deadline_ms: at + 1000,
            },
        },
        at,
    )
    .unwrap();
    for (name, mut_key, revision, when) in [
        ("stale-revision", key.clone(), 1, at),
        ("expired", key.clone(), 2, at + 1000),
        (
            "wrong-digest",
            EffectKey {
                digest: "foreign".into(),
                ..key.clone()
            },
            2,
            at,
        ),
        (
            "wrong-generation",
            {
                let mut k = key.clone();
                k.fence.attempt += 1;
                k
            },
            2,
            at,
        ),
    ] {
        assert!(
            effect_command_at(
                &owner,
                &run,
                name,
                EffectCommand::Answer {
                    key: mut_key,
                    revision,
                    approved: true
                },
                when
            )
            .is_err(),
            "{name}"
        );
    }
    let results = std::thread::scope(|scope| {
        [true, false]
            .into_iter()
            .map(|approved| {
                let owner = owner.clone();
                let run = run.clone();
                let key = key.clone();
                scope.spawn(move || {
                    loop {
                        let latest = owner.store.neutral_execution(&run.run.id).unwrap();
                        let result = effect_command_at(
                            &owner,
                            &latest,
                            if approved { "yes" } else { "no" },
                            EffectCommand::Answer {
                                key: key.clone(),
                                revision: 2,
                                approved,
                            },
                            at,
                        );
                        if result
                            .as_ref()
                            .is_err_and(|error| error.contains("sequence changed"))
                        {
                            continue;
                        }
                        break result;
                    }
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.contains("already answered"))
    );
    let latest = owner.store.neutral_execution(&run.run.id).unwrap();
    assert!(latest.effect(&key).unwrap().answer.is_some());
    assert_eq!(latest.sequence, run.sequence + 1);
    assert!(!dir.path().join("write").exists());
    // A valid invocation digest cannot be carried over to new resolved values.
    let mut changed = latest.effect(&key).unwrap().clone();
    changed.invocation.inputs.insert("value".into(), json!(9));
    assert_ne!(
        NeutralEffect::prepare(changed.invocation.clone(), changed.preparation.clone())
            .unwrap()
            .key
            .digest,
        key.digest
    );
    changed.invocation.inputs.insert("value".into(), json!(6));
    changed.invocation.subject = Some(BTreeMap::from([("artifact".into(), json!("changed"))]));
    let changed_subject = NeutralEffect::prepare(changed.invocation, changed.preparation).unwrap();
    assert!(
        effect_command_at(
            &owner,
            &latest,
            "changed-subject",
            EffectCommand::Answer {
                key: changed_subject.key,
                revision: 2,
                approved: true
            },
            at
        )
        .unwrap_err()
        .contains("digest")
    );
}

// Catches: cancellation or deadline releasing a conflicting resource without
// stop evidence, or a late callback reopening an ended graph.
#[tokio::test]
async fn cancelled_pending_effect_keeps_claim_until_proven_stop_without_reopening_graph() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host.clone());
    let pending = dispatch(&owner, &run.run.id).await;
    let key = only_key(&pending);
    assert_eq!(
        pending.effect(&key).unwrap().handle.as_deref(),
        Some("owned-operation-1")
    );
    let cancelled = owner
        .neutral_command(&run.run.id, "cancel", pending.sequence, Command::Cancel)
        .unwrap();
    assert_eq!(cancelled.status, NeutralStatus::Cancelled);
    assert_eq!(claims(&owner), 1);
    let requested = owner
        .neutral_effect_command(&run.run.id, EffectCommand::Cancel { key: key.clone() })
        .unwrap();
    assert!(requested.effect(&key).unwrap().cancel_started);
    let mut result = FileHost::result(requested.effect(&key).unwrap());
    result.stopped = false;
    let not_stopped = owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: result,
            },
        )
        .unwrap();
    assert_eq!(claims(&owner), 1);
    assert!(not_stopped.attempt.as_ref().unwrap().result.is_none());
    let stopped = owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: FileHost::result(not_stopped.effect(&key).unwrap()),
            },
        )
        .unwrap();
    assert_eq!(stopped.status, NeutralStatus::Cancelled);
    assert!(stopped.outputs.is_none());
    assert_eq!(claims(&owner), 0);
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    assert!(
        std::fs::read_to_string(&host.file)
            .unwrap()
            .contains(&key.digest),
        "cancel is not rollback"
    );
}

// Catches: first-slot approval head-of-line blocking, uncertainty permitting a
// new conflicting mutation, or conflict locks incorrectly blocking diagnosis.
#[tokio::test]
async fn branch_local_question_and_uncertain_claim_leave_read_only_sibling_runnable() {
    for approval in [true, false] {
        let (dir, owner, run, _) = super::neutral_scope_tests::fixture("collection", 1);
        let (run, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
        let host = FileHost::new(
            &owner,
            dir.path().join("write"),
            if approval {
                Reply::Complete
            } else {
                Reply::Lost
            },
            approval,
        );
        owner.neutral_host(host.clone());
        let notify = Arc::new(Notify::new());
        // Prepare then dispatch/skip each current candidate through the actual
        // owner. No question, uncertainty or claim is hand-set in the snapshot.
        for _ in 0..12 {
            let (snapshot, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
            if !owner.dispatch_neutral(&snapshot, &notify).unwrap() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let snapshot = owner.store.neutral_execution(&run.run.id).unwrap();
        let diagnostic = snapshot
            .effects
            .values()
            .find(|effect| effect.invocation.inputs["value"] == json!(20))
            .unwrap();
        assert!(
            diagnostic.dispatched,
            "read-only diagnosis must pass the waiting mutation"
        );
        let mutation = snapshot
            .effects
            .values()
            .find(|effect| effect.invocation.inputs["value"] != json!(20))
            .unwrap();
        if approval {
            assert!(mutation.question.is_some());
            assert!(!mutation.dispatched);
        } else {
            assert_eq!(
                snapshot
                    .effects
                    .values()
                    .filter(|effect| effect.preparation.mutation && effect.dispatched)
                    .count(),
                1
            );
            assert_eq!(claims(&owner), 1);
            assert!(
                snapshot
                    .effects
                    .values()
                    .any(|effect| effect.preparation.mutation && !effect.dispatched),
                "the conflicting candidate must be prepared, not merely absent due to concurrency"
            );
        }
        let mut stale = diagnostic.key.clone();
        stale.fence.scope.as_mut().unwrap().identity.generation += 1;
        assert!(
            owner
                .neutral_effect_command(
                    &run.run.id,
                    EffectCommand::Observe {
                        key: stale,
                        observation: FileHost::result(diagnostic)
                    }
                )
                .is_err()
        );
    }
}

// Catches: expiry treating a cancel request as stop evidence, or cancellation
// being lost because a terminal actor exits before calling the owned host.
#[tokio::test]
async fn expired_actor_requests_cancel_but_does_not_release_without_stop_evidence() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host.clone());
    let pending = dispatch(&owner, &run.run.id).await;
    let expired = owner
        .neutral_command_at(
            &run.run.id,
            "deadline",
            pending.sequence,
            Command::Step,
            pending.deadline_ms,
        )
        .unwrap();
    assert_eq!(expired.status, NeutralStatus::Expired);
    let notify = Arc::new(Notify::new());
    super::runtime::run_actor(Arc::downgrade(&owner), run.run.id.clone(), notify.clone()).await;
    notify.notified().await;
    let stopped_unknown = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(stopped_unknown.status, NeutralStatus::Expired);
    assert_eq!(host.stops.load(Ordering::Acquire), 1);
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    assert_eq!(claims(&owner), 1);
    assert!(stopped_unknown.outputs.is_none());
    assert!(
        stopped_unknown
            .effects
            .values()
            .all(|effect| effect.cancel_started)
    );
}

// Catches: a run-local lock allowing a conflicting second run to invoke, or a
// known failed result being confused with uncertainty and holding locks forever.
#[tokio::test]
async fn resource_projection_blocks_other_runs_but_known_stopped_failure_releases() {
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host.clone());
    let pending = dispatch(&owner, &run.run.id).await;
    let key = only_key(&pending);
    let workflows =
        crate::workflows::WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let mut catalog = crate::workflows::catalog::WorkflowCatalog::new();
    let mut handlers = BTreeSet::new();
    for descriptor in &run.run.closure.descriptors {
        handlers.insert(descriptor.handler.clone());
        catalog.register(descriptor.clone()).unwrap();
    }
    let mut request = run.run.request.clone();
    request.request_id = "other-run".into();
    let other = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    let other = owner.store.neutral_execution(&other.id).unwrap();
    let other = prepared(&owner, &other, at);
    let notify = Arc::new(Notify::new());
    assert!(owner.dispatch_neutral(&other, &notify).unwrap()); // commits intent, not dispatch
    let waiting = owner.store.neutral_execution(&other.run.id).unwrap();
    assert!(!owner.dispatch_neutral(&waiting, &notify).unwrap());
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    let failed = owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key,
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Failed {
                            diagnostic: "owned operation definitively failed".into(),
                        },
                    },
                },
            },
        )
        .unwrap();
    assert!(matches!(failed.status, NeutralStatus::Failed { .. }));
    assert!(failed.outputs.is_none());
    assert_eq!(claims(&owner), 0);
    assert!(owner.dispatch_neutral(&waiting, &notify).unwrap());
    notify.notified().await;
    assert_eq!(host.calls.load(Ordering::Acquire), 2);
    assert_eq!(claims(&owner), 1);
}

// Catches: recursive ancestor accounting reading an old parent-frame copy of
// effect certainty, which withholds a real diagnostic dispatch permit.
#[tokio::test]
async fn nested_diagnosis_uses_current_global_uncertainty_not_frozen_parent_effects() {
    let (dir, owner, run, _) =
        super::neutral_runtime_tests::subflow_fixture_with_ancestor(false, 2, false, true);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host.clone());
    let notify = Arc::new(Notify::new());
    let (root, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    assert!(owner.dispatch_neutral(&root, &notify).unwrap());
    let dispatched = owner.store.neutral_execution(&run.run.id).unwrap();
    let key = only_key(&dispatched);
    for _ in 0..2 {
        let (snapshot, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
        assert!(owner.dispatch_neutral(&snapshot, &notify).unwrap());
    }
    let nested = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(nested.execution.matches('/').count(), 2);
    owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key,
                observation: EffectObservation {
                    stopped: false,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "parent response lost after nested entry".into(),
                    },
                },
            },
        )
        .unwrap();
    // Keep the owned futures unpolled on this current-thread runtime, so both
    // child calls must be admitted before either supplies a completion.
    for _ in 0..4 {
        let (snapshot, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
        if !owner.dispatch_neutral(&snapshot, &notify).unwrap() {
            break;
        }
    }
    assert_eq!(
        host.calls.load(Ordering::Acquire),
        3,
        "one original mutation and both independent nested diagnostic calls"
    );
    assert_eq!(
        claims(&owner),
        1,
        "the uncertain mutation still owns its conflicting resource"
    );
    tokio::task::yield_now().await;
}

#[path = "neutral_effect_regression_tests.rs"]
mod regressions;

#[path = "neutral_effect_orphan_tests.rs"]
mod orphan_regression;

#[path = "neutral_model_tool_tests.rs"]
mod model_tools;
