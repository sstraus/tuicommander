//! Trigger crash recovery and supersede ordering, per source, through the real
//! `RuntimeOwner` and its durable intent ledger. A restart drops the owner and
//! acquires the same run database again; nothing is hand-set.
use super::neutral_effects::{
    EffectCommand, EffectKey, EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect,
};
use super::neutral_execution::{NeutralExecutionCommand as Command, NeutralResult, NeutralStatus};
use super::neutral_runtime::NeutralInvocation;
use super::neutral_trigger_fixture::{Fixture, event, fixture, pr_event};
use super::neutral_triggers::trigger_request;
use super::runtime::RuntimeOwner;
use crate::workflows::{scheduled_trigger::TriggerSchedule, trigger::*};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
enum Source {
    Github,
    Gitlab,
    Scheduled,
}

/// Enable one binding of `source` and return the event its source would submit.
fn arm(f: &Fixture, source: Source) -> TriggerEvent {
    let mut draft = f.draft.clone();
    match source {
        Source::Github => {
            f.enable(draft, 0);
            event()
        }
        Source::Gitlab => {
            draft.id = "binding-gitlab".into();
            draft.source = TriggerSource::Gitlab;
            draft.kind = "merge_request".into();
            draft.resource_prefix = "gitlab:test:mr:".into();
            draft.events = BTreeSet::from(["opened".into()]);
            draft.mapping.inputs = BTreeMap::from([("initial".into(), "mr".into())]);
            let event = TriggerEvent {
                source: TriggerSource::Gitlab,
                kind: "merge_request".into(),
                resource: "gitlab:test:mr:7".into(),
                generation: "head-1".into(),
                event: "opened".into(),
                delivery_id: Some("delivery-1".into()),
                schema: trigger_schema(TriggerSource::Gitlab, "merge_request").unwrap(),
                values: BTreeMap::from([
                    ("instance".into(), json!("https://gitlab.example.invalid")),
                    ("project".into(), json!(1)),
                    ("generation".into(), json!("head-1")),
                    ("action".into(), json!("open")),
                    ("mr".into(), json!(7)),
                    ("head".into(), json!("a".repeat(40))),
                ]),
                actor: None,
            };
            enable_for(f, draft, &event);
            event
        }
        Source::Scheduled => {
            draft.id = "binding-sched".into();
            draft.source = TriggerSource::Scheduled;
            draft.kind = "schedule".into();
            draft.resource_prefix = "schedule:binding-sched".into();
            draft.events = BTreeSet::from(["due".into()]);
            draft.schedule = Some(TriggerSchedule {
                cron: "* * * * *".into(),
                timezone: "UTC".into(),
                text: "tick".into(),
            });
            draft.mapping.inputs = BTreeMap::from([("initial".into(), "occurrence_ms".into())]);
            let event = TriggerEvent {
                source: TriggerSource::Scheduled,
                kind: "schedule".into(),
                resource: "schedule:binding-sched".into(),
                generation: "1:1700000000000".into(),
                event: "due".into(),
                delivery_id: None,
                schema: trigger_schema(TriggerSource::Scheduled, "schedule").unwrap(),
                values: BTreeMap::from([
                    ("text".into(), json!("tick")),
                    ("occurrence_ms".into(), json!(1_700_000_000_000_i64)),
                ]),
                actor: None,
            };
            enable_for(f, draft, &event);
            event
        }
    }
}

/// Save and enable a non-GitHub binding, previewing with its own source event.
fn enable_for(f: &Fixture, draft: TriggerBindingDraft, event: &TriggerEvent) {
    let saved = f
        .workflows
        .save_neutral_trigger_binding("/project", 0, draft, &f.catalog, &f.handlers)
        .unwrap();
    let overlaps = f
        .workflows
        .preview_neutral_trigger_mapping("/project", &saved.draft, event, &f.catalog, &f.handlers)
        .unwrap()
        .overlaps;
    f.workflows
        .enable_neutral_trigger_binding(
            "/project",
            &saved.draft.id,
            saved.revision,
            true,
            &overlaps,
            &f.catalog,
            &f.handlers,
        )
        .unwrap();
}

/// Stop the host and bring it back on the same durable run database.
fn restart(f: Fixture) -> Fixture {
    let Fixture {
        _dir,
        owner,
        workflows,
        catalog,
        handlers,
        draft,
    } = f;
    drop(owner);
    let owner = RuntimeOwner::acquire(&_dir.path().join("runs.sqlite3")).unwrap();
    Fixture {
        _dir,
        owner,
        workflows,
        catalog,
        handlers,
        draft,
    }
}

fn trigger_runs(f: &Fixture) -> Vec<String> {
    f.owner
        .store
        .neutral_runs_with_request_prefix("/project", "trigger:")
        .unwrap()
        .into_iter()
        .map(|(run, _)| run)
        .collect()
}

/// The durable intent of the first binding matching `event`, as the source
/// retry would reserve it, before any run starts.
fn reserve(f: &Fixture, event: &TriggerEvent) -> super::neutral_trigger_store::TriggerIntent {
    let binding = f
        .workflows
        .stored_neutral_trigger_bindings("/project")
        .unwrap()
        .into_iter()
        .find(|b| b.enabled && matches(&b.draft, event))
        .unwrap();
    let request = trigger_request(
        &f.workflows,
        &f.catalog,
        &f.handlers,
        "/project",
        &binding,
        event,
    )
    .unwrap();
    f.owner
        .store
        .reserve_trigger("/project", &binding.draft.id, event.clone(), request, "")
        .unwrap()
}

// Catches: a crash after the intent is reserved and before any run exists
// consuming the event zero times (the retry sees an "already reserved" intent
// and starts nothing) or twice.
#[test]
fn crash_after_reserve_then_replay_starts_exactly_one_run_for_every_source() {
    for source in [Source::Github, Source::Gitlab, Source::Scheduled] {
        let f = fixture();
        let event = arm(&f, source);
        let intent = reserve(&f, &event);
        assert_eq!(intent.admission.disposition, "pending", "{source:?}");
        assert!(
            trigger_runs(&f).is_empty(),
            "{source:?}: nothing started yet"
        );
        let f = restart(f);
        let admitted = f.admit(event.clone());
        let runs = trigger_runs(&f);
        assert_eq!(runs.len(), 1, "{source:?}: exactly one run, got {runs:?}");
        assert_eq!(admitted.len(), 1, "{source:?}");
        assert_eq!(admitted[0].run_id.as_ref(), Some(&runs[0]), "{source:?}");
        assert_eq!(admitted[0].disposition, "started", "{source:?}");
        // A second source retry after the link is a no-op.
        assert_eq!(f.admit(event)[0].run_id.as_ref(), Some(&runs[0]));
        assert_eq!(trigger_runs(&f).len(), 1, "{source:?}");
    }
}

// Catches: a crash after the run is created and before `trigger_started` links
// the receipt starting a second run on retry instead of re-adopting the first.
#[test]
fn crash_between_run_start_and_receipt_link_adopts_the_same_run_for_every_source() {
    for source in [Source::Github, Source::Gitlab, Source::Scheduled] {
        let f = fixture();
        let event = arm(&f, source);
        let intent = reserve(&f, &event);
        let orphan = f
            .owner
            .start_trigger(&f.workflows, &f.catalog, &f.handlers, &intent.request)
            .unwrap();
        // The receipt is not linked: the ledger still shows the event pending.
        assert_eq!(
            f.owner
                .store
                .trigger_intent(&intent.receipt)
                .unwrap()
                .unwrap()
                .admission
                .run_id,
            None,
            "{source:?}"
        );
        let f = restart(f);
        let admitted = f.admit(event);
        assert_eq!(
            trigger_runs(&f),
            vec![orphan.id.clone()],
            "{source:?}: the retry re-adopts the orphan, never a second run"
        );
        assert_eq!(admitted[0].run_id.as_ref(), Some(&orphan.id), "{source:?}");
        assert_eq!(admitted[0].disposition, "started", "{source:?}");
    }
}

/// Run `id` to a dispatched write on the PR resource. `observe_uncertain`
/// leaves the effect uncertain; otherwise it stays dispatched with no answer.
fn hold_write(f: &Fixture, id: &str, observe_uncertain: bool) -> EffectKey {
    for (step, command) in [("enter", Command::Step), ("prepare", Command::Step)] {
        let snapshot = f.owner.store.neutral_execution(id).unwrap();
        f.owner
            .neutral_command(id, step, snapshot.sequence, command)
            .unwrap();
    }
    let snapshot = f.owner.store.neutral_execution(id).unwrap();
    let attempt = snapshot.attempt.as_ref().unwrap();
    let effect = NeutralEffect::prepare(
        NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: snapshot.publication.clone(),
            descriptor: snapshot.descriptor(0).unwrap().clone(),
            instance: snapshot.publication.definition.instances[0].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        },
        EffectPreparation {
            mutation: true,
            resources: ["github:octo-org/example/pull_request/1347".into()].into(),
            approval: None,
            model_tools: None,
        },
    )
    .unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            id,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    if observe_uncertain {
        f.owner
            .neutral_effect_command(
                id,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: EffectObservation {
                        stopped: false,
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: "response lost".into(),
                        },
                    },
                },
            )
            .unwrap();
    }
    key
}

fn settle_write(f: &Fixture, id: &str, key: &EffectKey) {
    f.owner
        .neutral_effect_command(
            id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: "ok".into(),
                            outputs: BTreeMap::from([("next".into(), json!(7))]),
                        },
                    },
                },
            },
        )
        .unwrap();
}

fn pr_binding(f: &Fixture) {
    let mut draft = f.draft.clone();
    draft.kind = "pull_request".into();
    draft.mapping.inputs.insert("head".into(), "head".into());
    f.enable(draft, 0);
}

// Catches: after a restart, the supersede path forgetting the old run's
// unsettled write (the effect ledger is not consulted again) and starting the
// newest head while the old write may still land; and a lost middle head
// being started late instead of staying coalesced. Runs the matrix over both
// unsettled shapes: dispatched without an answer, and answered uncertain.
#[test]
fn newest_generation_waits_for_unsettled_write_across_a_restart_and_middle_never_starts() {
    for uncertain in [false, true] {
        let f = fixture();
        pr_binding(&f);
        let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
        let key = hold_write(&f, &first, uncertain);
        // Cancel by supersede (generation 2), then a third generation, then the host dies.
        assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
        assert!(f.admit(pr_event("3", 'c'))[0].run_id.is_none());
        let f = restart(f);
        // The source retries the newest head after the restart: still held.
        assert!(
            f.admit(pr_event("3", 'c'))[0].run_id.is_none(),
            "uncertain={uncertain}: newest head must not start over an unsettled write"
        );
        assert_eq!(
            trigger_runs(&f),
            vec![first.clone()],
            "uncertain={uncertain}"
        );
        assert_eq!(
            f.owner.store.neutral_execution(&first).unwrap().status,
            NeutralStatus::Cancelled,
            "uncertain={uncertain}: the old run was cancelled by the supersede"
        );
        settle_write(&f, &first, &key);
        let latest = f.admit(pr_event("3", 'c'))[0].run_id.clone().unwrap();
        let runs = trigger_runs(&f);
        assert_eq!(
            runs.len(),
            2,
            "uncertain={uncertain}: old run + newest only: {runs:?}"
        );
        assert_eq!(
            f.owner.store.neutral_run(&latest).unwrap().inputs["head"],
            json!("c".repeat(40)),
            "uncertain={uncertain}: the run carries the newest head"
        );
    }
}

fn disposition(f: &Fixture, generation: &str, head: char) -> String {
    f.owner
        .store
        .trigger_original(
            "/project",
            "binding-a",
            &pr_event(generation, head).resource,
            generation,
        )
        .expect("generation stays in the ledger")
        .admission
        .disposition
}

// Catches: the middle generation staying `pending` forever once a newer
// generation owns the resource, so the Designer shows a head that can never
// start as still waiting.
#[test]
fn coalesced_middle_generation_ends_superseded_without_a_run() {
    let f = fixture();
    pr_binding(&f);
    let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    let key = hold_write(&f, &first, true);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    assert_eq!(disposition(&f, "2", 'b'), "pending", "no successor yet");
    assert!(f.admit(pr_event("3", 'c'))[0].run_id.is_none());
    assert_eq!(disposition(&f, "2", 'b'), "superseded");
    assert_eq!(disposition(&f, "3", 'c'), "pending", "the successor waits");
    settle_write(&f, &first, &key);
    f.admit(pr_event("3", 'c'));
    assert_eq!(disposition(&f, "2", 'b'), "superseded");
    assert_eq!(disposition(&f, "3", 'c'), "started");
    let middle = f
        .owner
        .store
        .trigger_original("/project", "binding-a", &pr_event("2", 'b').resource, "2")
        .unwrap();
    assert!(middle.admission.run_id.is_none());
}

// Catches: a superseded intent re-enqueued by a source retry after a restart
// (it would displace the newest head and start a stale run).
#[test]
fn superseded_intent_replayed_after_restart_stays_superseded_and_does_not_requeue() {
    let f = fixture();
    pr_binding(&f);
    let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    let key = hold_write(&f, &first, true);
    f.admit(pr_event("2", 'b'));
    f.admit(pr_event("3", 'c'));
    let f = restart(f);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    assert_eq!(disposition(&f, "2", 'b'), "superseded");
    assert_eq!(
        disposition(&f, "3", 'c'),
        "pending",
        "newest keeps the slot"
    );
    settle_write(&f, &first, &key);
    let latest = f.admit(pr_event("3", 'c'))[0].run_id.clone().unwrap();
    assert_eq!(
        f.owner.store.neutral_run(&latest).unwrap().inputs["head"],
        json!("c".repeat(40))
    );
    assert_eq!(
        trigger_runs(&f).len(),
        2,
        "old run + newest, never the middle"
    );
    assert_eq!(disposition(&f, "2", 'b'), "superseded");
}

// Catches: marking an intent superseded when nothing replaced it (a held
// single successor must stay pending across a restart until it starts).
#[test]
fn single_pending_event_without_successor_stays_pending_until_started() {
    let f = fixture();
    pr_binding(&f);
    let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    let key = hold_write(&f, &first, true);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    let f = restart(f);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    assert_eq!(disposition(&f, "2", 'b'), "pending");
    settle_write(&f, &first, &key);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_some());
    assert_eq!(disposition(&f, "2", 'b'), "started");
}

// Catches: an operator-resolved write (resources released, never `settled`)
// still holding the pending newest head for ever, because trigger admission
// asked `settled` instead of `resources_released`.
#[test]
fn operator_resolved_write_releases_the_pending_newest_generation() {
    let f = fixture();
    pr_binding(&f);
    let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    let key = hold_write(&f, &first, false);
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    f.owner
        .neutral_effect_command(
            &first,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "response lost".into(),
                    },
                },
            },
        )
        .unwrap();
    assert!(
        f.admit(pr_event("2", 'b'))[0].run_id.is_none(),
        "an unresolved uncertain write still holds the head"
    );
    f.owner
        .neutral_effect_command(
            &first,
            EffectCommand::ResolveMutation {
                key,
                evidence: "Verified the original request stopped and reconciled the pull request"
                    .into(),
                stop_proof: super::neutral_effects::MutationStopProof::RecordedObservation,
            },
        )
        .unwrap();
    let latest = f.admit(pr_event("2", 'b'))[0].run_id.clone();
    assert!(latest.is_some(), "a resolved write must release the head");
}
