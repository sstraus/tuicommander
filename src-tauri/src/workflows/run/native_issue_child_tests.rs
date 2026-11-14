//! Original-child observation/cancellation never redispatches its handoff.
use super::native_issue_effect::IssueAction;
use super::native_issue_service::*;
use super::native_issue_tests::{approved, setup};
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectKey, EffectObservation, EffectOutcome};
use super::neutral_runtime::NeutralHost;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct ChildOwner {
    receipt: IssueChildReceipt,
    starts: AtomicUsize,
    observes: AtomicUsize,
    cancels: AtomicUsize,
}
impl IssueServices for ChildOwner {
    fn triage<'a>(
        &'a self,
        _: &'a super::native_host::NativeBinding,
        _: &'a EffectKey,
        _: &'a super::native_issue_read::IssueReadReceipt,
        _: &'a IssueProfile,
    ) -> IssueFuture<'a, ScopedTriage> {
        Box::pin(async { Err("no triage in child lifecycle scenario".into()) })
    }
    fn handoff<'a>(
        &'a self,
        _: &'a super::native_host::NativeBinding,
        _: &'a EffectKey,
        _: &'a IssueChildRequest,
    ) -> IssueFuture<'a, IssueChildReceipt> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err("no new dispatch during observation".into()) })
    }
    fn observe<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        self.observes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(self.receipt.clone()) })
    }
    fn cancel<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            let mut receipt = self.receipt.clone();
            receipt.status = IssueChildStatus::Cancelled;
            Ok(receipt)
        })
    }
}
// Catches: recovery starting a second child; dropping source/publication on
// cancellation; accepting another child id with otherwise matching metadata.
#[tokio::test]
async fn native_issue_child_observe_and_cancel_keep_original_identity_without_dispatch() {
    let f = Fixture::new();
    let (base, seed, bound) = setup(
        &f,
        IssueAction::Comment {
            body: "unused".into(),
        },
    );
    let reference = EffectKey {
        fence: seed.pending_invocations()[0].1.fence.clone(),
        digest: "triage-placeholder".into(),
    };
    let request = IssueChildRequest {
        source: base.source,
        triage: reference,
        publication: seed.run.request.publication,
        inputs: Default::default(),
    };
    let run = f.start("issue.handoff", json!(request));
    let intent = bound
        .store
        .trigger_intent(&request.source.receipt)
        .unwrap()
        .unwrap();
    bound.store.trigger_started(&intent, &run.run.id).unwrap();
    let run = approved(&f, &run).await;
    let effect = run.effects.values().next().unwrap();
    let run = f
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: EffectObservation {
                    stopped: false,
                    outcome: EffectOutcome::Pending {
                        handle: "original-child".into(),
                    },
                },
            },
        )
        .unwrap();
    let effect = run.effects.values().next().unwrap().clone();
    let owner = Arc::new(ChildOwner {
        receipt: IssueChildReceipt {
            source: request.source.clone(),
            triage: request.triage.clone(),
            publication: request.publication.clone(),
            child: "original-child".into(),
            status: IssueChildStatus::Completed,
            evidence: Some("Owned child completion evidence".into()),
        },
        starts: AtomicUsize::new(0),
        observes: AtomicUsize::new(0),
        cancels: AtomicUsize::new(0),
    });
    f.host.install_issue_services(owner.clone());
    let observed = f.host.reconcile(effect.clone()).await.unwrap();
    assert!(observed.stopped);
    let output = match observed.outcome {
        EffectOutcome::Completed {
            result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
        } => outputs,
        _ => panic!("expected original completion"),
    };
    assert_eq!(output["receipt"]["child"], "original-child");
    assert_eq!(output["receipt"]["source"], json!(request.source));
    let cancelled = f.host.cancel(effect.clone()).await.unwrap();
    assert!(cancelled.stopped);
    assert_eq!(owner.starts.load(Ordering::SeqCst), 0);
    assert_eq!(owner.observes.load(Ordering::SeqCst), 1);
    assert_eq!(owner.cancels.load(Ordering::SeqCst), 1);
    let mut replaced = owner.receipt.clone();
    replaced.child = "replacement-child".into();
    f.host.install_issue_services(Arc::new(ChildOwner {
        receipt: replaced,
        starts: AtomicUsize::new(0),
        observes: AtomicUsize::new(0),
        cancels: AtomicUsize::new(0),
    }));
    assert!(matches!(
        f.host.reconcile(effect).await.unwrap().outcome,
        EffectOutcome::Uncertain { .. }
    ));
}
