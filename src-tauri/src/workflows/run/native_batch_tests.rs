use super::super::check::git_output;
use super::super::native_batch::{self as batch, Critic, ONE_SLOT};
use super::super::native_batch_evidence as evidence;
use super::super::native_cohort::{Cohort, StoryPin};
use super::super::native_test_support::Fixture;
use serde_json::json;
use std::path::Path;

fn cohort() -> Cohort {
    Cohort {
        plan: "plan".into(),
        plan_revision: 1,
        source: "source".into(),
        stories: ["A", "B"]
            .map(|id| StoryPin {
                id: id.into(),
                revision: 1,
            })
            .into(),
    }
}
fn commit(path: &Path, text: &str) -> String {
    std::fs::write(path.join("artifact.txt"), text).unwrap();
    git_output(path, &["add", "."]).unwrap();
    tuic_git::git::git_commit_blocking(
        path.to_string_lossy().into_owned(),
        text.into(),
        Some(false),
    )
    .unwrap();
    git_output(path, &["rev-parse", "HEAD"]).unwrap()
}
fn head(path: &Path) -> (String, String) {
    super::super::check::clean_artifact(path).unwrap()
}
fn err<T: std::fmt::Debug>(result: Result<T, String>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.contains(expected), "{error}");
}

// Catches a second member starting before the first lands, an out-of-order start,
// a duplicate child start and a replayed effect recording twice.
#[test]
fn native_batch_barrier_admits_in_order_one_slot_and_refuses_duplicate_starts() {
    let f = Fixture::new();
    let (s, c) = (&f.owner.store, cohort());
    err(batch::admit(s, "run", "e1", &c, "B", &[]), "in order");
    let first = batch::admit(s, "run", "e2", &c, "A", &["p0".into()]).unwrap();
    assert_eq!(first, json!({"plan":"plan","story":"A","generation":1}));
    // A redelivered admission returns its receipt although the slot is now held.
    assert_eq!(batch::admit(s, "run", "e2", &c, "A", &[]).unwrap(), first);
    err(batch::admit(s, "run", "e3", &c, "B", &[]), ONE_SLOT);
    let mut changed = c.clone();
    changed.source = "other".into();
    err(
        batch::admit(s, "run", "e4", &changed, "A", &[]),
        "cannot change",
    );
    err(
        batch::land(s, "run", "e5", "plan", "B", "p1", "c1"),
        "no admitted",
    );
    err(
        batch::land(s, "run", "e6", "plan", "A", "p0", "c0"),
        "predates",
    );
    batch::land(s, "run", "e7", "plan", "A", "p1", "c1").unwrap();
    err(
        batch::admit(s, "run", "e8", &c, "A", &[]),
        "already started",
    );
    batch::admit(s, "run", "e9", &c, "B", &["p0".into()]).unwrap();
    err(
        batch::land(s, "run", "e10", "plan", "B", "p1", "c1"),
        "already credited",
    );
    // Another run keeps its own ledger.
    batch::admit(s, "other", "e1", &c, "A", &[]).unwrap();
    assert_eq!(batch::retained(s, "run", "e5").unwrap(), None);
}

// Catches a pin while a slot is held, a member left out or added, a landing the
// pinned commit does not contain and a dirty artifact.
#[test]
fn native_batch_pin_holds_exact_landed_members_at_a_clean_containing_artifact() {
    let fixture = Fixture::new();
    let (s, c, path) = (
        &fixture.owner.store,
        cohort(),
        fixture.context.workspace_path(),
    );
    let a = commit(path, "a");
    batch::admit(s, "run", "a1", &c, "A", &[]).unwrap();
    batch::land(s, "run", "a2", "plan", "A", "pa", &a).unwrap();
    batch::admit(s, "run", "b1", &c, "B", &[]).unwrap();
    err(batch::pin(s, "run", "p1", "plan", path), "has not landed");
    let b = commit(path, "b");
    batch::land(s, "run", "b2", "plan", "B", "pb", &b).unwrap();
    std::fs::write(path.join("artifact.txt"), "dirty").unwrap();
    err(batch::pin(s, "run", "p2", "plan", path), "clean");
    git_output(path, &["checkout", "--", "artifact.txt"]).unwrap();
    let pinned = batch::pin(s, "run", "p3", "plan", path).unwrap();
    let (commit_id, tree) = head(path);
    assert_eq!(pinned["generation"], 1);
    assert_eq!(pinned["commit"], commit_id);
    assert_eq!(pinned["tree"], tree);
    assert_eq!(
        pinned["members"],
        json!([{"story":"A","generation":1,"commit":a},{"story":"B","generation":1,"commit":b}])
    );
    assert_eq!(batch::pin(s, "run", "p3", "plan", path).unwrap(), pinned);
    err(batch::pin(s, "run", "p4", "plan", path), "no landed member");
    // A landing outside the pinned lineage cannot be pinned.
    let orphan = git_output(path, &["commit-tree", &tree, "-m", "orphan"]).unwrap();
    batch::admit(s, "foreign", "a1", &c, "A", &[]).unwrap();
    batch::land(s, "foreign", "a2", "plan", "A", "px", &orphan).unwrap();
    err(
        batch::pin(s, "foreign", "p1", "plan", path),
        "does not contain",
    );
}

// Catches a second verdict, a FIX naming a non-member, a stale push releasing the
// re-entry join, a partial cohort reported complete and an unapplied FIX hidden.
#[test]
fn native_batch_verdict_routes_fix_reentry_and_never_completes_a_partial_cohort() {
    let fixture = Fixture::new();
    let (s, c, path) = (
        &fixture.owner.store,
        cohort(),
        fixture.context.workspace_path(),
    );
    let critic = |report: &str, fix: Option<&str>| {
        let (commit, tree) = head(path);
        Critic {
            report: report.into(),
            commit,
            tree,
            fix: fix.map(Into::into),
        }
    };
    let a = commit(path, "a");
    batch::admit(s, "run", "a1", &c, "A", &[]).unwrap();
    batch::land(s, "run", "a2", "plan", "A", "pa", &a).unwrap();
    batch::pin(s, "run", "p1", "plan", path).unwrap();
    let partial = batch::verdict(s, "run", "v1", "plan", 1, &critic("r1", None)).unwrap();
    assert_eq!(partial["outcome"], "continue");
    assert_eq!(partial["remaining"], json!(["B"]));
    assert_eq!(
        partial["repeat"], true,
        "a partial cohort must repeat the configured loop"
    );
    err(
        batch::verdict(s, "run", "v2", "plan", 1, &critic("r2", None)),
        "single review",
    );
    batch::admit(s, "run", "b1", &c, "B", &["pa".into()]).unwrap();
    let b = commit(path, "b");
    batch::land(s, "run", "b2", "plan", "B", "pb", &b).unwrap();
    batch::pin(s, "run", "p2", "plan", path).unwrap();
    err(
        batch::verdict(s, "run", "v3", "plan", 1, &critic("r3", None)),
        "latest",
    );
    let mut other = critic("r4", None);
    other.tree = "another".into();
    err(
        batch::verdict(s, "run", "v4", "plan", 2, &other),
        "another artifact",
    );
    err(
        batch::verdict(s, "run", "v5", "plan", 2, &critic("r5", Some("A"))),
        "member of",
    );
    let fix = batch::verdict(s, "run", "v6", "plan", 2, &critic("r6", Some("B"))).unwrap();
    assert_eq!(fix["outcome"], "fix");
    assert_eq!(
        fix["repeat"], true,
        "an armed FIX must repeat the configured loop"
    );
    // The FIX owns the next slot; nothing can be pinned or approved around it.
    err(batch::pin(s, "run", "p3", "plan", path), "has not landed");
    err(batch::admit(s, "run", "a3", &c, "A", &[]), "armed FIX");
    let reentry = batch::admit(s, "run", "b3", &c, "B", &["pa".into(), "pb".into()]).unwrap();
    assert_eq!(reentry["generation"], 2);
    err(
        batch::land(s, "run", "b4", "plan", "B", "pb", &b),
        "predates",
    );
    let fixed = commit(path, "b fixed");
    batch::land(s, "run", "b5", "plan", "B", "pb2", &fixed).unwrap();
    let third = batch::pin(s, "run", "p4", "plan", path).unwrap();
    assert_eq!(
        third["members"],
        json!([{"story":"B","generation":2,"commit":fixed}])
    );
    let done = batch::verdict(s, "run", "v7", "plan", 3, &critic("r7", None)).unwrap();
    assert_eq!(done["outcome"], "completed");
    assert_eq!(done["remaining"], json!([]));
    assert_eq!(
        done["repeat"], false,
        "a complete approved cohort must stop the configured loop"
    );
}

// Catches a foreign-run or unapproved push landing a member, a non-push effect as a
// landing, a non-reviewer role as the critic and a report over another artifact.
#[tokio::test]
async fn native_batch_evidence_requires_owned_settled_push_and_exact_reviewer_report() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
    let path = f.context.workspace_path();
    let (commit_id, tree) = head(path);
    let bare = f.dir.path().join("landing.git");
    std::fs::create_dir(&bare).unwrap();
    git_output(&bare, &["init", "--bare"]).unwrap();
    let push = f.start(
        "publication.push",
        json!({"commit":commit_id,"tree":tree,
        "remote":bare.to_string_lossy(),"target":"refs/heads/landed"}),
    );
    let waiting = f.dispatch(&push).await;
    let key = waiting.effects.values().next().unwrap().key.clone();
    assert!(
        evidence::landing(&waiting, &key)
            .unwrap_err()
            .contains("no completed")
    );
    let pushed = f.dispatch(&f.approve(&waiting)).await;
    assert_eq!(evidence::landing(&pushed, &key).unwrap(), commit_id);
    assert_eq!(evidence::known_pushes(&pushed), vec![key.digest.clone()]);
    let mut foreign = key.clone();
    foreign.fence.run = "foreign".into();
    assert!(
        evidence::landing(&pushed, &foreign)
            .unwrap_err()
            .contains("foreign run")
    );
    // Decision 1(i): a native commit of this run also lands its member; the same
    // commit seen from another run is no landing evidence.
    std::fs::write(path.join("artifact.txt"), "landed by commit").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let staged = git_output(path, &["write-tree"]).unwrap();
    let review = super::super::native_review_tests::reviewed(
        &f,
        crate::workflows::review_verdict::CodeVerdict::Pass,
    )
    .await;
    let commit_run = f.start(
        "commit.create",
        json!({"parent":commit_id,"tree":staged,"message":"land","review":review}),
    );
    let waiting = f.dispatch(&commit_run).await;
    let committed = f.dispatch(&f.approve(&waiting)).await;
    let commit_key = committed.effects.values().next().unwrap().key.clone();
    assert_eq!(
        evidence::landing(&committed, &commit_key).unwrap(),
        git_output(path, &["rev-parse", "HEAD"]).unwrap()
    );
    assert_eq!(
        evidence::known_pushes(&committed),
        vec![commit_key.digest.clone()]
    );
    assert!(
        evidence::landing(&pushed, &commit_key)
            .unwrap_err()
            .contains("foreign run")
    );
    // Decision G1: the landing is selected from the run's own ledger; one known at
    // admission is never chosen, and nothing new means no landing.
    assert_eq!(
        evidence::select_landing(&committed, &[]).unwrap(),
        (
            commit_key.digest.clone(),
            git_output(path, &["rev-parse", "HEAD"]).unwrap()
        )
    );
    assert!(
        evidence::select_landing(&committed, std::slice::from_ref(&commit_key.digest))
            .unwrap_err()
            .contains("since the member's admission")
    );
    let pinned = f.dispatch(&f.start("workspace.pin", json!({}))).await;
    let pin_key = pinned.effects.values().next().unwrap().key.clone();
    assert!(
        evidence::landing(&pinned, &pin_key)
            .unwrap_err()
            .contains("native push")
    );

    let (protocol, runtime) = super::super::neutral_model_adapter_tests::fixture(
        "{\"decision\":\"fix\",\"story\":\"B\"}",
        false,
    );
    for entry in std::fs::read_dir(protocol.path()).unwrap() {
        let file = entry.unwrap().path();
        std::fs::copy(&file, path.join(file.file_name().unwrap())).unwrap();
    }
    git_output(path, &["add", "."]).unwrap();
    commit(path, "protocol fixture");
    let mut request = super::super::neutral_model_adapter_tests::request();
    request.output_schema = serde_json::from_value(json!({"fields":{
        "decision":{"value_type":"string","required":true,"default":null},
        "story":{"value_type":"string","required":false,"default":null}}}))
    .unwrap();
    request.limits.max_output_bytes = 256;
    let report = |role: &str| {
        let request = json!(request);
        let run = super::super::native_agent_test_support::role_run_custom(
            &f,
            role,
            &runtime,
            super::super::NativeCaller::Owner,
            None,
            |d| {
                d.instances[0].config.insert("request".into(), request);
            },
        );
        let source: super::super::native_agent::Source =
            serde_json::from_value(run.inputs["source"].clone()).unwrap();
        (run, source)
    };
    let (run, source) = report("reviewer");
    let run = f.dispatch(&run).await;
    let effect = run.effects.values().next().unwrap();
    assert!(effect.settled(), "{effect:?}");
    let bound = f.host.binding(&effect.invocation).unwrap();
    let critic = evidence::critic(&bound, &run.run.id, &effect.key, &source).unwrap();
    assert_eq!(critic.fix.as_deref(), Some("B"));
    assert_eq!(
        (critic.commit.as_str(), critic.tree.as_str()),
        (source.commit.as_str(), source.tree.as_str())
    );
    assert!(evidence::critic(&bound, "foreign", &effect.key, &source).is_err());
    let mut stale = source.clone();
    stale.tree = "another".into();
    assert!(evidence::critic(&bound, &run.run.id, &effect.key, &stale).is_err());
    let (run, source) = report("implementer");
    let run = f.dispatch(&run).await;
    let effect = run.effects.values().next().unwrap();
    let bound = f.host.binding(&effect.invocation).unwrap();
    let refused = evidence::critic(&bound, &run.run.id, &effect.key, &source);
    assert!(refused.err().unwrap().contains("reviewer or validator"));
}

// Catches replaying already landed members, skipping the armed FIX owner, or
// replacing the retained cohort with a smaller per-round cohort.
#[test]
fn native_batch_pending_selects_bounded_unstarted_members_and_fix_before_new_work() {
    let f = Fixture::new();
    let (store, path) = (&f.owner.store, f.context.workspace_path());
    let mut cohort = cohort();
    cohort.stories = (0..10)
        .map(|n| StoryPin {
            id: format!("story-{n}"),
            revision: 1,
        })
        .collect();
    let selected = batch::pending(store, "run", &cohort, 8).unwrap();
    assert_eq!(
        selected.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        [
            "story-0", "story-1", "story-2", "story-3", "story-4", "story-5", "story-6", "story-7"
        ]
    );
    assert!(batch::pending(store, "run", &cohort, 0).is_err());
    assert!(batch::pending(store, "run", &cohort, 129).is_err());
    for (i, story) in selected.iter().enumerate() {
        batch::admit(store, "run", &format!("admit-{i}"), &cohort, &story.id, &[]).unwrap();
        err(batch::pending(store, "run", &cohort, 8), ONE_SLOT);
        let commit = commit(path, &format!("member-{i}"));
        batch::land(
            store,
            "run",
            &format!("land-{i}"),
            &cohort.plan,
            &story.id,
            &format!("push-{i}"),
            &commit,
        )
        .unwrap();
    }
    let rest = batch::pending(store, "run", &cohort, 8).unwrap();
    assert_eq!(
        rest.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["story-8", "story-9"]
    );
    assert_eq!(
        batch::pending(store, "another-run", &cohort, 8).unwrap(),
        selected
    );
    let mut changed = cohort.clone();
    changed.stories = rest;
    err(batch::pending(store, "run", &changed, 8), "cannot change");
    let pin = batch::pin(store, "run", "pin", &cohort.plan, path).unwrap();
    let critic = Critic {
        report: "critic".into(),
        commit: pin["commit"].as_str().unwrap().into(),
        tree: pin["tree"].as_str().unwrap().into(),
        fix: Some("story-3".into()),
    };
    batch::verdict(store, "run", "verdict", &cohort.plan, 1, &critic).unwrap();
    let fix = batch::pending(store, "run", &cohort, 8).unwrap();
    assert_eq!(fix, vec![cohort.stories[3].clone()]);
}
