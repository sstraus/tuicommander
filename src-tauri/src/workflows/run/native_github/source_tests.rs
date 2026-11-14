use super::*;
use crate::workflows::run::{
    check::git_output,
    native_test_support::Fixture,
    neutral::{NeutralLimits, NeutralStartRequest},
};
use crate::workflows::{
    github_trigger::{GithubResource, GithubSnapshot},
    neutral_publication::PublicationRef,
};
use std::collections::{BTreeMap, BTreeSet};
// Catches a retained approval publishing after a new source generation replaces its run.
#[tokio::test]
async fn review_publication_source_generation_replacement_refuses() {
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let request = GitHubRequest {
        source_intent: None,
        account: "default".into(),
        repository: "owner/repo".into(),
        commit: git_output(path, &["rev-parse", "HEAD"]).unwrap(),
        tree: git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap(),
        action: GitHubAction::MarkReady { number: 3 },
    };
    let run = f.start(
        "publication.github",
        serde_json::to_value(&request).unwrap(),
    );
    assert!(
        source::current(&f.owner.store, &run.run.id, &request)
            .unwrap()
            .is_none()
    );
    let mut snapshot = GithubSnapshot {
        repository: request.repository.clone(),
        number: 3,
        kind: GithubResource::PullRequest,
        open: true,
        title: "Review".into(),
        material_digest: "material".into(),
        labels: BTreeSet::new(),
        head: Some(request.commit.clone()),
        base: Some("b".repeat(40)),
        updated_at: "2026-11-11T12:00:00Z".into(),
        actor: None,
    };
    let event = f
        .owner
        .store
        .normalize_github(snapshot.clone(), None)
        .unwrap()
        .remove(0);
    let start = NeutralStartRequest {
        publication: PublicationRef {
            project: run.publication.project.clone(),
            id: run.publication.id.clone(),
            revision: run.publication.revision,
        },
        request_id: "pending".into(),
        inputs: BTreeMap::new(),
        subject: None,
        limits: NeutralLimits {
            max_activations: 20,
            max_concurrency: 1,
            max_duration_ms: 600000,
        },
    };
    let intent = f
        .owner
        .store
        .reserve_trigger(&run.publication.project, "binding", event, start, "")
        .unwrap();
    f.owner.store.trigger_started(&intent, &run.run.id).unwrap();
    // Fixture attaches the already persisted run to a real reserved trigger; this
    // proves the publication gate, not public ingress/start integration.
    f.owner
        .store
        .trigger_connection()
        .unwrap()
        .execute(
            "UPDATE neutral_workflow_runs SET request_id=?2 WHERE id=?1",
            rusqlite::params![run.run.id, format!("trigger:{}", intent.receipt)],
        )
        .unwrap();
    assert_eq!(
        source::current(&f.owner.store, &run.run.id, &request)
            .unwrap()
            .unwrap()
            .event
            .generation,
        intent.event.generation
    );
    let mut wrong = request.clone();
    wrong.action = GitHubAction::MarkReady { number: 4 };
    assert!(source::current(&f.owner.store, &run.run.id, &wrong).is_err());
    snapshot.head = Some("c".repeat(40));
    snapshot.updated_at = "2026-11-11T12:01:00Z".into();
    f.owner.store.normalize_github(snapshot, None).unwrap();
    assert!(source::current(&f.owner.store, &run.run.id, &request).is_err());
}

// Catches: an issue trigger acting as blanket MarkReady authority, or borrowing
// another artifact/account/provider number. The provider receipt is controlled;
// this tests retained provenance, not a network publication.
#[tokio::test]
async fn issue_mark_ready_requires_exact_same_run_stopped_draft_receipt() {
    use crate::workflows::run::{
        native_issue_tests,
        neutral_effects::{EffectCommand, EffectObservation, EffectOutcome},
        neutral_execution::NeutralResult,
    };
    let f = Fixture::new();
    let (source, intent, _) = native_issue_tests::admission(&f);
    let open = GitHubRequest {
        account: source.account.clone(),
        repository: source.repository.clone(),
        commit: git_output(f.context.workspace_path(), &["rev-parse", "HEAD"]).unwrap(),
        tree: git_output(f.context.workspace_path(), &["rev-parse", "HEAD^{tree}"]).unwrap(),
        action: GitHubAction::OpenPr {
            branch: "delivery".into(),
            base: "main".into(),
            title: "Issue delivery".into(),
            body: "Source-bound work".into(),
        },
        source_intent: None,
    };
    let run = f.start_triggered(
        "publication.github",
        serde_json::to_value(&open).unwrap(),
        &source.receipt,
    );
    f.owner.store.trigger_started(&intent, &run.run.id).unwrap();
    assert_eq!(
        source::current(&f.owner.store, &run.run.id, &open)
            .unwrap()
            .unwrap()
            .event
            .generation,
        source.generation
    );
    let mut ready = GitHubRequest {
        action: GitHubAction::MarkReady { number: 7 },
        ..open.clone()
    };
    let missing = source::current(&f.owner.store, &run.run.id, &ready).unwrap_err();
    assert!(missing.contains("exact stopped draft"), "{missing}");
    let run = native_issue_tests::approved(&f, &run).await;
    let key = run.effects.values().next().unwrap().key.clone();
    let receipt = GitHubReceipt {
        account: open.account.clone(),
        repository: open.repository.clone(),
        commit: open.commit.clone(),
        action: open.action.clone(),
        number: Some(7),
        url: Some("https://github.com/octo-org/example/pull/7".into()),
        source_generation: Some(source.generation.clone()),
    };
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key,
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: "ok".into(),
                            outputs: BTreeMap::from([(
                                "receipt".into(),
                                serde_json::to_value(receipt).unwrap(),
                            )]),
                        },
                    },
                },
            },
        )
        .unwrap();
    let source = source::current(&f.owner.store, &run.run.id, &ready)
        .unwrap()
        .unwrap();
    assert!(
        source::require_issue_ready_base(
            &f.owner.store,
            &run.run.id,
            &ready,
            Some(&source),
            &serde_json::json!({"base":{"ref":"main"}})
        )
        .is_ok()
    );
    assert!(
        source::require_issue_ready_base(
            &f.owner.store,
            &run.run.id,
            &ready,
            Some(&source),
            &serde_json::json!({"base":{"ref":"elsewhere"}})
        )
        .is_err()
    );
    for changed in [
        GitHubRequest {
            account: "other".into(),
            ..ready.clone()
        },
        GitHubRequest {
            commit: "f".repeat(40),
            ..ready.clone()
        },
        GitHubRequest {
            tree: "e".repeat(40),
            ..ready.clone()
        },
        GitHubRequest {
            action: GitHubAction::MarkReady { number: 8 },
            ..ready.clone()
        },
    ] {
        assert!(source::current(&f.owner.store, &run.run.id, &changed).is_err());
    }
    ready.action = GitHubAction::Comment {
        number: 7,
        body: "Unrelated write".into(),
    };
    assert!(
        source::current(&f.owner.store, &run.run.id, &ready)
            .unwrap_err()
            .contains("does not authorize")
    );
}
