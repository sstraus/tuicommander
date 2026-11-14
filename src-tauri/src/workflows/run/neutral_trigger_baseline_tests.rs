use super::{RunStore, neutral_trigger_fixture::*};
use crate::workflows::{github_event::from_rest, github_trigger::GithubResource};

// Catches: enabling a binding immediately replaying every old open resource.
#[test]
fn enabling_a_binding_does_not_admit_preexisting_open_resources() {
    use super::runtime::RuntimeOwner;
    use crate::workflows::trigger::ReplayAuthority;
    use serde_json::json;
    use std::collections::BTreeMap;
    for kind in [GithubResource::Issue, GithubResource::PullRequest] {
        let f = fixture();
        let mut draft = f.draft.clone();
        if kind == GithubResource::PullRequest {
            draft.kind = "pull_request".into();
            draft.events = ["updated".into()].into();
        }
        let binding = f.enable_without_baseline(draft.clone(), 0);
        let event = if kind == GithubResource::Issue {
            event()
        } else {
            pr_event("1", 'a')
        };
        assert!(
            f.admit(event.clone()).is_empty(),
            "pre-existing resource started before baseline"
        );
        let mut raw: serde_json::Value = serde_json::from_str(match kind {
            GithubResource::Issue => include_str!("../fixtures/github-issue-rest.json"),
            GithubResource::PullRequest => include_str!("../fixtures/github-pr-rest.json"),
        })
        .unwrap();
        let snapshot = from_rest("octo-org/example", kind, &raw).unwrap();
        assert_eq!(
            f.owner
                .admit_github_batch(
                    &f.workflows,
                    &f.catalog,
                    &f.handlers,
                    "/project",
                    "octo-org/example",
                    kind,
                    vec![snapshot.clone()]
                )
                .unwrap(),
            [] as [crate::workflows::trigger::TriggerAdmission; 0]
        );
        let baseline = f
            .owner
            .store
            .trigger_original("/project", "binding-a", &event.resource, "1")
            .unwrap();
        assert_eq!(baseline.admission.disposition, "baseline");
        assert!(baseline.admission.run_id.is_none());
        assert!(
            f.owner
                .store
                .trigger_pending("/project")
                .unwrap()
                .is_empty()
        );
        // Release/reacquire the real owner: baseline receipts and activation cut
        // survive a daemon restart, not just re-reading an in-memory marker.
        let Fixture {
            _dir,
            owner,
            workflows,
            catalog,
            handlers,
            ..
        } = f;
        drop(owner);
        let owner = RuntimeOwner::acquire(&_dir.path().join("runs.sqlite3")).unwrap();
        assert!(
            owner
                .store
                .trigger_baseline_ready("/project", "binding-a", binding.revision)
                .unwrap()
        );
        assert_eq!(
            owner
                .admit_github_batch(
                    &workflows,
                    &catalog,
                    &handlers,
                    "/project",
                    "octo-org/example",
                    kind,
                    vec![snapshot.clone()]
                )
                .unwrap(),
            [] as [crate::workflows::trigger::TriggerAdmission; 0]
        );
        assert!(
            owner
                .replay_trigger(
                    &workflows,
                    &catalog,
                    &handlers,
                    "/project",
                    "binding-a",
                    &event.resource,
                    "1",
                    "explicit",
                    &ReplayAuthority::authenticated_owner("/other")
                )
                .is_err()
        );
        let replay = owner
            .replay_trigger(
                &workflows,
                &catalog,
                &handlers,
                "/project",
                "binding-a",
                &event.resource,
                "1",
                "explicit",
                &ReplayAuthority::authenticated_owner("/project"),
            )
            .unwrap();
        assert!(replay[0].run_id.is_some());
        assert_eq!(
            owner
                .store
                .neutral_run(replay[0].run_id.as_ref().unwrap())
                .unwrap()
                .inputs["initial"],
            json!(1347)
        );
        // A later new binding cannot consume the old retained event outbox.
        draft.id = "binding-b".into();
        let saved = workflows
            .save_neutral_trigger_binding("/project", 0, draft, &catalog, &handlers)
            .unwrap();
        workflows
            .enable_neutral_trigger_binding(
                "/project",
                "binding-b",
                saved.revision,
                true,
                &BTreeMap::from([("binding-a".into(), binding.revision)]),
                &catalog,
                &handlers,
            )
            .unwrap();
        assert_eq!(
            owner
                .admit_github_batch(
                    &workflows,
                    &catalog,
                    &handlers,
                    "/project",
                    "octo-org/example",
                    kind,
                    vec![snapshot]
                )
                .unwrap(),
            [] as [crate::workflows::trigger::TriggerAdmission; 0]
        );
        raw["updated_at"] = json!("2026-11-11T10:00:01Z");
        if kind == GithubResource::Issue {
            raw["body"] = json!("Later material");
        } else {
            raw["head"]["sha"] = json!("b".repeat(40));
        }
        let updated = from_rest("octo-org/example", kind, &raw).unwrap();
        // Crash window: normalized durable source event exists before admission.
        owner.store.normalize_github(updated.clone(), None).unwrap();
        let later = owner
            .admit_github_batch(
                &workflows,
                &catalog,
                &handlers,
                "/project",
                "octo-org/example",
                kind,
                vec![updated],
            )
            .unwrap();
        assert_eq!(
            later.len(),
            2,
            "later generation must reach both ready bindings"
        );
        assert!(later.iter().all(|a| a.run_id.is_some()));
        raw["updated_at"] = json!("2026-11-11T10:00:02Z");
        if kind == GithubResource::Issue {
            raw["body"] = json!("Lost source");
        } else {
            raw["head"]["sha"] = json!("c".repeat(40));
        }
        owner
            .store
            .normalize_github(from_rest("octo-org/example", kind, &raw).unwrap(), None)
            .unwrap();
        owner
            .store
            .retire_github_source("octo-org/example", kind, 1347)
            .unwrap();
        assert!(
            owner
                .admit_github_batch(
                    &workflows,
                    &catalog,
                    &handlers,
                    "/project",
                    "octo-org/example",
                    kind,
                    Vec::new()
                )
                .unwrap()
                .is_empty(),
            "unavailable source outbox must not auto-admit a stale snapshot"
        );
    }
}

// Catches: an unchanged poll producing the previously retained opened/label
// event again, which replays the backlog into a later new binding.
#[test]
fn unchanged_snapshot_does_not_re_emit_retained_event() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = RunStore::open_at(&dir.path().join("runs.sqlite3")).unwrap();
    let snapshot = from_rest(
        "octo-org/example",
        GithubResource::Issue,
        &serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap(),
    )
    .unwrap();
    assert_ne!(
        store.normalize_github(snapshot.clone(), None).unwrap(),
        [] as [crate::workflows::trigger::TriggerEvent; 0]
    );
    assert!(
        store.normalize_github(snapshot, None).unwrap().is_empty(),
        "unchanged snapshot re-emitted retained event"
    );
}

// Catches: neutral_trigger_self_events never written in production, so a run's
// own label write re-emits a labeled event that admits a second run (and
// supersedes the run that wrote it); and a self-write claim suppressing a
// generation that also carries a foreign label or material change.
#[test]
fn run_own_label_write_records_self_event_and_starts_no_second_run() {
    use serde_json::json;
    let f = fixture();
    f.enable(f.draft.clone(), 0);
    let mut raw: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap();
    let batch = |raw: &serde_json::Value| {
        f.owner
            .admit_github_batch(
                &f.workflows,
                &f.catalog,
                &f.handlers,
                "/project",
                "octo-org/example",
                GithubResource::Issue,
                vec![from_rest("octo-org/example", GithubResource::Issue, raw).unwrap()],
            )
            .unwrap()
    };
    let first = batch(&raw);
    let run = first[0]
        .run_id
        .clone()
        .expect("precondition: the issue starts a run");
    let resource = "github:octo-org/example/issue/1347";
    let claim = |generation: &str, label: &str| {
        let intent = f
            .owner
            .store
            .trigger_original("/project", "binding-a", resource, generation)
            .unwrap();
        // What publish_with records before its label POST.
        f.owner
            .store
            .record_self_label(&intent.receipt, label, "effect")
            .unwrap();
    };
    claim("1", "Accepted");
    raw["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"accepted"}));
    raw["updated_at"] = json!("2026-11-11T10:00:01Z");
    assert!(
        batch(&raw).is_empty(),
        "the run's own label write admitted a second run"
    );
    let labeled = f
        .owner
        .store
        .retained_github_events("octo-org/example", GithubResource::Issue, false)
        .unwrap()
        .into_iter()
        .find(|e| e.event == "labeled")
        .unwrap();
    assert_eq!(labeled.generation, "2");
    assert!(
        f.owner
            .store
            .trigger_suppressed("/project", "binding-a", &labeled)
            .unwrap()
    );
    assert_eq!(
        f.owner.store.neutral_execution(&run).unwrap().status,
        super::neutral_execution::NeutralStatus::Running,
        "the run's own write superseded it"
    );

    // A label someone else adds is a real event.
    raw["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"urgent"}));
    raw["updated_at"] = json!("2026-11-11T10:00:02Z");
    assert_eq!(batch(&raw).len(), 1, "a foreign label was suppressed");

    // A claimed label that arrives with a material change is not only ours.
    claim("3", "done");
    raw["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"done"}));
    raw["body"] = json!("Edited requirements");
    raw["updated_at"] = json!("2026-11-11T10:00:03Z");
    assert!(
        !batch(&raw).is_empty(),
        "a claim suppressed a foreign material change"
    );
}
