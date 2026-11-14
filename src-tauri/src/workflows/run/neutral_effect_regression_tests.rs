use super::*;

fn intent(
    owner: &RuntimeOwner,
    run: &NeutralExecution,
    value: i64,
    deadline: Option<i64>,
    at: i64,
) -> (NeutralExecution, EffectKey) {
    let (activity, attempt) = run
        .pending_invocations()
        .into_iter()
        .find(|(_, attempt)| attempt.inputs["value"] == json!(value))
        .unwrap();
    let effect = NeutralEffect::prepare(
        NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: run.publication.clone(),
            descriptor: run.descriptor(activity).unwrap().clone(),
            instance: run.publication.definition.instances[activity].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        },
        EffectPreparation {
            model_tools: None,
            mutation: true,
            resources: BTreeSet::from(["owned-test-file".into()]),
            approval: deadline.map(|deadline_ms| EffectQuestion {
                revision: 1,
                title: "Exact write?".into(),
                deadline_ms,
            }),
        },
    )
    .unwrap();
    let key = effect.key.clone();
    let next = effect_command_at(
        owner,
        run,
        &format!("intent-{value}"),
        EffectCommand::Prepare {
            effect: Box::new(effect),
        },
        at,
    )
    .unwrap();
    (next, key)
}

async fn finish(owner: &Arc<RuntimeOwner>, id: &str) -> NeutralExecution {
    let notify = Arc::new(Notify::new());
    for _ in 0..16 {
        let (snapshot, _) = owner.drive_neutral_turn(id).unwrap();
        owner.dispatch_neutral(&snapshot, &notify).unwrap();
        tokio::task::yield_now().await;
    }
    owner.store.neutral_execution(id).unwrap()
}

// Catches: an approved effect queued behind a resource outliving its question
// aborts dispatch forever instead of failing locally and permitting diagnosis.
#[tokio::test]
async fn expired_approved_effect_settles_and_later_sibling_runs() {
    let (dir, owner, run, at) = super::super::neutral_scope_tests::fixture("collection", 2);
    let (run, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    let (run, expired) = intent(&owner, &run, 30, Some(at + 1000), at);
    let run = effect_command_at(
        &owner,
        &run,
        "approve",
        EffectCommand::Answer {
            key: expired.clone(),
            revision: 1,
            approved: true,
        },
        at,
    )
    .unwrap();
    let (run, holding) = intent(&owner, &run, 10, None, at);
    let run = effect_command_at(
        &owner,
        &run,
        "hold-resource",
        EffectCommand::Dispatch {
            key: holding.clone(),
        },
        at,
    )
    .unwrap();
    assert!(
        effect_command_at(
            &owner,
            &run,
            "queued",
            EffectCommand::Dispatch {
                key: expired.clone(),
            },
            at
        )
        .unwrap_err()
        .contains("resource conflict")
    );
    let run = effect_command_at(
        &owner,
        &run,
        "expired-dispatch",
        EffectCommand::Dispatch {
            key: expired.clone(),
        },
        at + 1000,
    )
    .unwrap();
    assert!(!run.effect(&expired).unwrap().dispatched);
    let held = run.effect(&holding).unwrap();
    effect_command_at(
        &owner,
        &run,
        "holding-stopped",
        EffectCommand::Observe {
            key: holding.clone(),
            observation: FileHost::result(held),
        },
        at + 1000,
    )
    .unwrap();
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Complete, false);
    owner.neutral_host(host.clone());
    let finished = finish(&owner, &run.run.id).await;
    let items = &finished.outputs.as_ref().unwrap().values["items"];
    assert_eq!(items[0]["state"], json!("failed"));
    assert_eq!(items[2]["outputs"]["next"], json!(21));
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    assert!(!host.file.exists(), "expired mutation must never invoke");
    assert_eq!(claims(&owner), 0);
}

struct BadPreparationHost {
    inner: Arc<FileHost>,
    fault: u8,
}
impl NeutralHost for BadPreparationHost {
    fn context(&self, invocation: &NeutralInvocation) -> Result<Box<dyn HostContext>, String> {
        self.inner.context(invocation)
    }
    fn registry(&self) -> HostRegistry<'_> {
        self.inner.registry()
    }
    fn prepare(&self, invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let mut preparation = self.inner.prepare(invocation)?;
        if invocation.inputs["value"] == json!(30) {
            match self.fault {
                0 => return Err("owned preparation refused".into()),
                1 => preparation.resources.clear(),
                2 => {
                    preparation.approval = Some(EffectQuestion {
                        revision: 1,
                        title: "Invalid bound".into(),
                        deadline_ms: self
                            .inner
                            .owner
                            .upgrade()
                            .unwrap()
                            .store
                            .neutral_execution(&invocation.fence.run)?
                            .deadline_ms
                            + 1,
                    })
                }
                _ => unreachable!(),
            }
        }
        Ok(preparation)
    }
    fn invoke_effect(&self, effect: NeutralEffect) -> ObservationFuture {
        self.inner.invoke_effect(effect)
    }
}

// Catches: host/shape/ledger preparation errors retrying the same branch forever
// and starving independently runnable siblings before any adapter invocation.
#[tokio::test]
async fn preparation_refusals_settle_and_later_sibling_runs() {
    for fault in 0..3 {
        let (dir, owner, run, _) = super::super::neutral_scope_tests::fixture("collection", 2);
        let inner = FileHost::new(&owner, dir.path().join("write"), Reply::Complete, false);
        owner.neutral_host(Arc::new(BadPreparationHost {
            inner: inner.clone(),
            fault,
        }));
        let finished = finish(&owner, &run.run.id).await;
        let items = &finished.outputs.as_ref().unwrap().values["items"];
        assert_eq!(items[0]["state"], json!("failed"), "fault {fault}");
        assert_eq!(items[2]["outputs"]["next"], json!(21));
        assert_eq!(inner.calls.load(Ordering::Acquire), 2);
        assert_eq!(claims(&owner), 0);
    }
}

// Catches: revising a declined question resetting its winning answer and
// admitting a contradictory approval after that attempt already failed.
#[test]
fn declined_effect_cannot_reopen_with_new_revision() {
    let (_dir, owner, run, at) = super::super::neutral_scope_tests::fixture("collection", 2);
    let (run, _) = owner.drive_neutral_turn(&run.run.id).unwrap();
    let (run, key) = intent(&owner, &run, 30, Some(at + 1000), at);
    let declined = effect_command_at(
        &owner,
        &run,
        "decline",
        EffectCommand::Answer {
            key: key.clone(),
            revision: 1,
            approved: false,
        },
        at,
    )
    .unwrap();
    assert_eq!(declined.status, NeutralStatus::Running);
    assert!(
        effect_command_at(
            &owner,
            &declined,
            "reopen",
            EffectCommand::Question {
                key: key.clone(),
                question: EffectQuestion {
                    revision: 2,
                    title: "Try again?".into(),
                    deadline_ms: at + 1000,
                },
            },
            at
        )
        .is_err()
    );
    assert!(
        effect_command_at(
            &owner,
            &declined,
            "approve-resolved",
            EffectCommand::Answer {
                key: key.clone(),
                revision: 1,
                approved: true,
            },
            at
        )
        .is_err()
    );
    let replayed = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(replayed.effect(&key).unwrap().answer, Some(false));
    assert_eq!(replayed.sequence, declined.sequence);
}
