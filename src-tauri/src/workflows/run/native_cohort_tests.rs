use super::native_cohort::Cohort;
use super::native_cohort_test_support::{child, cohort, parent};
use super::native_test_support::Fixture;
use super::neutral_execution::NeutralStatus;
use super::neutral_runtime::NeutralHost;
use crate::stories::{StoryCommand, StoryStore};
use crate::workflows::catalog::{DescriptorId, content_digest};
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::PublicationPin;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Notify;

// Catches generic terminal/subflow success standing in for native story
// completion, partial cohorts completing, and redelivery creating another child.
#[tokio::test]
async fn native_cohort_dispatch_joins_real_ordered_story_observations() {
    for completed in [1, 2] {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().into());
        let f = Fixture::new();
        let cohort = cohort(&f, completed);
        let (workflows, _, pin) = child(&f, &cohort);
        let original_pin = pin.clone();
        let run = parent(&f, &cohort, &workflows, pin);
        assert!(
            run.run
                .closure
                .children
                .iter()
                .any(|p| p.id == original_pin.reference.id
                    && p.revision == original_pin.reference.revision)
        );
        let current_catalog = workflows
            .build_project_catalog(
                f.context.workspace_path().to_str().unwrap(),
                &f.host
                    .catalog(f.context.workspace_path().to_str().unwrap())
                    .unwrap(),
                &f.host.registry(),
            )
            .unwrap();
        let handlers = f.host.registry().available_handlers();
        let old = workflows
            .get_neutral_publication(
                &original_pin.reference.project,
                &original_pin.reference.id,
                1,
                &current_catalog,
                &handlers,
            )
            .unwrap();
        let newer = workflows
            .save_neutral_publication(
                &old.project,
                &old.id,
                1,
                old.definition,
                &current_catalog,
                &handlers,
            )
            .unwrap();
        assert_eq!(newer.revision, 2);
        super::runtime::run_actor(
            Arc::downgrade(&f.owner),
            run.run.id.clone(),
            Arc::new(Notify::new()),
        )
        .await;
        let finished = f.owner.store.neutral_execution(&run.run.id).unwrap();
        let outcome = if completed == 2 {
            "completed"
        } else {
            "blocked"
        };
        assert_eq!(
            finished.status,
            NeutralStatus::Completed {
                outcome: outcome.into()
            }
        );
        let receipt = &finished.outputs.as_ref().unwrap().values["receipt"];
        assert_eq!(receipt["outcome"], outcome);
        assert_eq!(receipt["cohort"], json!(cohort));
        assert_eq!(receipt["owner"]["fence"]["run"], run.run.id);
        assert_eq!(receipt["integration_barrier"], "not_attested");
        for obligation in [
            "combined_artifact",
            "independent_batch_review",
            "fix_reentry",
        ] {
            assert_eq!(receipt["integration_obligations"][obligation], "blocked");
        }
        assert_eq!(finished.effects.len(), 1);
        assert!(
            finished
                .effects
                .values()
                .all(|e| e.settled() && e.invocation.publication.revision == 1)
        );
    }
}

// Catches stale cohort revisions, foreign plan scope, latest-child substitution,
// and a model/constant receipt bypassing the authoritative completion observer.
#[tokio::test]
async fn native_cohort_refuses_stale_foreign_or_unobserved_child_receipts() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let mut cohort = cohort(&f, 0);
    let (workflows, catalog, pin) = child(&f, &cohort);
    let handlers = f.host.registry().available_handlers();
    let published = workflows
        .get_neutral_publication(
            &pin.reference.project,
            &pin.reference.id,
            pin.reference.revision,
            &catalog,
            &handlers,
        )
        .unwrap();
    let id = DescriptorId {
        namespace: "user".into(),
        name: "pinned".into(),
        version: 1,
    };
    let mut wrong_pin = pin.clone();
    wrong_pin.sha256 = "wrong".into();
    assert!(
        crate::workflows::native_child::descriptor(
            id.clone(),
            wrong_pin,
            &published,
            &catalog,
            Some(cohort.clone())
        )
        .is_err()
    );
    let mut unobserved = published.clone();
    unobserved.definition.output_bindings.insert(
        "receipt".into(),
        DataSource::Constant {
            value: json!({"outcome":"completed"}),
        },
    );
    let changed_pin = PublicationPin {
        sha256: content_digest(&unobserved).unwrap(),
        ..pin.clone()
    };
    assert!(
        crate::workflows::native_child::descriptor(
            id,
            changed_pin,
            &unobserved,
            &catalog,
            Some(cohort.clone())
        )
        .is_err()
    );
    let backend = StoryStore::open().unwrap();
    let foreign = Fixture::new();
    assert!(cohort.current(&foreign.context, &backend, false).is_err());
    let first = backend.get_story(&cohort.stories[0].id).unwrap();
    backend
        .transition(&first.id, first.revision, StoryCommand::StartManual)
        .unwrap();
    assert!(cohort.current(&f.context, &backend, true).is_err());
    cohort.plan_revision += 1;
    assert!(cohort.current(&f.context, &backend, false).is_err());
    cohort.plan_revision -= 1;
    cohort.stories.push(cohort.stories[0].clone());
    assert!(cohort.validate().is_err());
}

// Catches a run-time cohort (decision a, 2026-11-12) admitting a story this run did
// not create, a stale story revision, or refusing the run's own created story.
#[tokio::test]
async fn native_dynamic_cohort_admits_only_stories_this_run_created_at_current_revision() {
    use crate::stories::{NewPlan, NewStory, StoryOrigin};
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let project = f.context.workspace_path().to_string_lossy().into_owned();
    let backend = StoryStore::open().unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project: project.clone(),
            title: "Dynamic cohort".into(),
            source: "plan-source".into(),
        })
        .unwrap();
    let new_story = |title: &str| NewStory {
        plan_id: plan.id.clone(),
        title: title.into(),
        criteria: vec!["Observed".into()],
        priority: 2,
        story_type: None,
        origin: StoryOrigin::Native,
        file_scope: vec![],
    };
    let request = json!({"review":"coordinator-1","cap":2,"source_story":null,"source_revision":null,
        "evidence":[],"proposal":"p1","story":new_story("Created by the run")});
    let run = f.start("story.create", request);
    let waiting = f.dispatch(&run).await;
    let done = f.dispatch(&f.approve(&waiting)).await;
    let effect = done.effects.values().next().unwrap();
    assert!(effect.settled(), "{effect:?}");
    let super::neutral_effects::EffectOutcome::Completed {
        result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
    } = &effect.observation.as_ref().unwrap().outcome
    else {
        panic!("{effect:?}")
    };
    let created = &outputs["receipt"]["story"];
    let foreign = backend
        .create_story(new_story("Created outside the run"))
        .unwrap();
    let context = f.host.context(&effect.invocation).unwrap();
    let context = (context.as_ref() as &dyn std::any::Any)
        .downcast_ref::<super::native_operations::NativeContext>()
        .unwrap()
        .clone();
    let plan = backend.get_plan(&plan.id).unwrap();
    let cohort = |stories: Vec<(String, i64)>| {
        json!({"plan":plan.id,"plan_revision":plan.revision,"source":"plan-source",
            "stories":stories.into_iter().map(|(id, revision)| json!({"id":id,"revision":revision})).collect::<Vec<_>>()})
    };
    let instance = crate::workflows::catalog::ActivityInstance {
        descriptor: DescriptorId {
            namespace: "user".into(),
            name: "dynamic-dispatch".into(),
            version: 1,
        },
        config: BTreeMap::new(),
        execution_policy: "native".into(),
    };
    let subject = BTreeMap::from([(
        "workspace".to_string(),
        json!({"reference":"workspace","id":project}),
    )]);
    let admit = |value: serde_json::Value| {
        super::native_cohort::admit(
            &context,
            &instance,
            &BTreeMap::from([("cohort".to_string(), value)]),
            Some(&subject),
        )
    };
    let created_pin = (
        created["id"].as_str().unwrap().to_owned(),
        created["revision"].as_i64().unwrap(),
    );
    admit(cohort(vec![created_pin.clone()])).unwrap();
    let refused = admit(cohort(vec![
        created_pin.clone(),
        (foreign.id.clone(), foreign.revision),
    ]))
    .unwrap_err();
    assert!(refused.contains("not created by this run"), "{refused}");
    backend
        .transition(&created_pin.0, created_pin.1, StoryCommand::StartManual)
        .unwrap();
    let stale = admit(cohort(vec![created_pin.clone()])).unwrap_err();
    assert!(stale.contains("stale"), "{stale}");
    // Catches a pending FIX/round passing its original stale revision to a child,
    // or refreshing the full admission cohort and thereby changing its ledger key.
    let original: Cohort = serde_json::from_value(cohort(vec![created_pin.clone()])).unwrap();
    let request = super::native_batch::PendingRequest {
        cohort: original.clone(),
        limit: 8,
    };
    let pending = super::native_batch_evidence::pending_receipt(
        &context,
        &f.owner.store,
        &effect.key.fence.run,
        &backend,
        &request,
    )
    .unwrap();
    assert_eq!(pending["cohort"], json!(original));
    assert_eq!(pending["members"][0]["cohort"], json!(original));
    let current = backend.get_story(&created_pin.0).unwrap();
    assert!(current.revision > created_pin.1);
    assert_eq!(
        pending["members"][0]["single"]["stories"][0]["revision"],
        current.revision
    );
    admit(pending["members"][0]["single"].clone()).unwrap();
    let saved = super::native_batch::retain_pending(
        &f.owner.store,
        &effect.key.fence.run,
        "pending-effect",
        pending.clone(),
    )
    .unwrap();
    assert_eq!(
        super::native_batch::retain_pending(
            &f.owner.store,
            &effect.key.fence.run,
            "pending-effect",
            json!({"changed":true})
        )
        .unwrap(),
        saved
    );
}

// Catches (decisions A(a) and B(a), 2026-11-12): native.story.cohort.created
// returning another run's story, and StartManual asking no question for a story
// this run did not create or did not admit to its batch slot.
#[tokio::test]
async fn native_created_cohort_and_start_coverage_stay_scoped_to_this_run() {
    use crate::stories::{NewPlan, NewStory, StoryOrigin};
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let project = f.context.workspace_path().to_string_lossy().into_owned();
    let backend = StoryStore::open().unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project: project.clone(),
            title: "Created cohort".into(),
            source: "plan-source".into(),
        })
        .unwrap();
    let create = |title: &str| {
        let story = NewStory {
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Observed".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        json!({"review":"coordinator-1","cap":2,"source_story":null,"source_revision":null,
            "evidence":[],"proposal":title,"story":story})
    };
    let mut created = Vec::new();
    for title in ["own", "foreign"] {
        let run = f.start("story.create", create(title));
        let waiting = f.dispatch(&run).await;
        let done = f.dispatch(&f.approve(&waiting)).await;
        let effect = done.effects.values().next().unwrap().clone();
        assert!(effect.settled(), "{effect:?}");
        created.push(effect);
    }
    let story_of = |e: &super::neutral_effects::NeutralEffect| {
        let super::neutral_effects::EffectOutcome::Completed {
            result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
        } = &e.observation.as_ref().unwrap().outcome
        else {
            panic!("{e:?}")
        };
        (
            outputs["receipt"]["story"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
            outputs["receipt"]["story"]["revision"].as_i64().unwrap(),
        )
    };
    let (own, own_revision) = story_of(&created[0]);
    let (foreign, foreign_revision) = story_of(&created[1]);
    let run = created[0].key.fence.run.clone();
    let request = |review: &str| super::native_cohort::CreatedRequest {
        plan: plan.id.clone(),
        review: review.into(),
    };
    let cohort = super::native_cohort::created_cohort(
        &f.context,
        &f.owner.store,
        &created[0].key,
        &backend,
        &request("coordinator-1"),
    )
    .unwrap();
    assert_eq!(
        cohort
            .stories
            .iter()
            .map(|s| (s.id.clone(), s.revision))
            .collect::<Vec<_>>(),
        [(own.clone(), own_revision)],
        "another run's story never enters this run's cohort"
    );
    assert!(
        super::native_cohort::created_cohort(
            &f.context,
            &f.owner.store,
            &created[0].key,
            &backend,
            &request("other-review")
        )
        .is_err()
    );
    let asks = |story: &str, revision: i64| {
        let transition = f.start(
            "story.transition",
            json!({"story":story,"revision":revision,"command":"start_manual"}),
        );
        let (activity, attempt) = transition.pending_invocations().into_iter().next().unwrap();
        let mut invocation = super::neutral_runtime::NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: transition.publication.clone(),
            descriptor: transition.descriptor(activity).unwrap().clone(),
            instance: transition.publication.definition.instances[activity].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        };
        invocation.fence.run = run.clone();
        f.host.prepare(&invocation).unwrap().approval.is_some()
    };
    assert!(
        asks(&own, own_revision),
        "created but not admitted still asks"
    );
    super::native_batch::admit(&f.owner.store, &run, "admit-own", &cohort, &own, &[]).unwrap();
    assert!(
        !asks(&own, own_revision),
        "created and admitted to this run's slot is covered"
    );
    // Admitting another run's story to this run's slot does not make it created here.
    let mut foreign_cohort = cohort.clone();
    foreign_cohort.plan = "foreign-plan".into();
    foreign_cohort.stories = vec![super::native_cohort::StoryPin {
        id: foreign.clone(),
        revision: foreign_revision,
    }];
    super::native_batch::admit(
        &f.owner.store,
        &run,
        "admit-foreign",
        &foreign_cohort,
        &foreign,
        &[],
    )
    .unwrap();
    assert!(
        asks(&foreign, foreign_revision),
        "another run's story still asks"
    );
}

// Catches (decision a, 2026-11-12): a native Reopen of a story that this run's batch
// ledger did not arm for FIX re-entry (not armed, or armed by another run) passing,
// and the armed re-entry not reopening the story as this run's workflow actor.
#[tokio::test]
async fn native_reopen_is_limited_to_this_runs_armed_fix_reentry() {
    use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStatus, StoryTransitionActor};
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let project = path.to_string_lossy().into_owned();
    let backend = StoryStore::open().unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project,
            title: "Re-entry".into(),
            source: "plan-source".into(),
        })
        .unwrap();
    let mut story = backend
        .create_story(NewStory {
            plan_id: plan.id.clone(),
            title: "Delivered".into(),
            criteria: vec!["Observed".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    for command in [
        StoryCommand::StartManual,
        StoryCommand::CheckCriterion(0),
        StoryCommand::SubmitReview,
        StoryCommand::Approve,
    ] {
        story = backend
            .transition(&story.id, story.revision, command)
            .unwrap();
    }
    let reopen = || json!({"story":story.id,"revision":story.revision,"command":"reopen"});
    let run = f.start("story.transition", reopen());
    let other = f.start("story.transition", reopen());
    let asks = |run: &super::neutral_execution::NeutralExecution| {
        let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
        let invocation = super::neutral_runtime::NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: run.publication.clone(),
            descriptor: run.descriptor(activity).unwrap().clone(),
            instance: run.publication.definition.instances[activity].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        };
        f.host.prepare(&invocation).unwrap().approval.is_some()
    };
    assert!(asks(&run), "not armed: Reopen keeps its approval");
    // Arm a FIX re-entry of this story in `run` through the real batch ledger.
    let (s, id) = (&f.owner.store, run.run.id.clone());
    let cohort = super::native_cohort::Cohort {
        plan: plan.id.clone(),
        plan_revision: backend.get_plan(&plan.id).unwrap().revision,
        source: "plan-source".into(),
        stories: vec![super::native_cohort::StoryPin {
            id: story.id.clone(),
            revision: story.revision,
        }],
    };
    let (commit, tree) = super::check::clean_artifact(path).unwrap();
    super::native_batch::admit(s, &id, "a1", &cohort, &story.id, &[]).unwrap();
    super::native_batch::land(s, &id, "l1", &plan.id, &story.id, "landing", &commit).unwrap();
    super::native_batch::pin(s, &id, "p1", &plan.id, path).unwrap();
    let critic = super::native_batch::Critic {
        report: "r1".into(),
        commit,
        tree,
        fix: Some(story.id.clone()),
    };
    super::native_batch::verdict(s, &id, "v1", &plan.id, 1, &critic).unwrap();
    assert!(!asks(&run), "this run's armed FIX re-entry is covered");
    assert!(asks(&other), "another run's Reopen still asks");
    let done = f.dispatch(&run).await;
    assert!(done.effects.values().next().unwrap().settled());
    assert_eq!(
        backend.get_story(&story.id).unwrap().status,
        StoryStatus::InProgress
    );
    let history = backend.transition_history(&story.id).unwrap();
    assert_eq!(
        history.last().unwrap().actor,
        StoryTransitionActor::Workflow { run_id: id }
    );
}
