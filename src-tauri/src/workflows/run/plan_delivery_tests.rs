//! Plan and story delivery through the public start, the production script
//! adapter and real Git. Only the agent commands are local test programs.
use super::native_host::NativeHost;
use super::native_test_support::Fixture;
use super::neutral_execution::{NeutralExecution, NeutralStatus};
use super::neutral_runtime::NeutralHost;
use super::plan_delivery::{Commands, install, story_input};
use super::runtime::RuntimeOwner;
use super::{NativeCaller, NativeContext, check::git_output};
use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStore};
use crate::workflows::WorkflowStore;
use crate::workflows::neutral_api::{model::NeutralAction, neutral_action};
use crate::workflows::neutral_host::NeutralCatalogHost;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Writes feature.txt: "fixed" once the prompt carries correction findings
/// (or always with `fixed` as the second argument), and counts its calls.
const IMPLEMENT: &str = "import sys\np=sys.stdin.read()\nopen(sys.argv[1],'a').write('call\\n')\nok=len(sys.argv)>2 or 'previous attempt was rejected' in p\nopen('feature.txt','w').write('fixed' if ok else 'broken')\n";
const VERIFY: &str = "import sys\nsys.exit(0 if open('feature.txt').read()=='fixed' else 1)\n";

fn python(script: &str, args: &[&str]) -> Vec<String> {
    let program = if cfg!(windows) { "python" } else { "python3" };
    [program, "-c", script]
        .into_iter()
        .chain(args.iter().copied())
        .map(String::from)
        .collect()
}

fn commands(log: &Path, fixed: bool) -> Commands {
    let log = log.to_str().unwrap();
    let args = if fixed { vec![log, "fixed"] } else { vec![log] };
    Commands {
        implement: python(IMPLEMENT, &args),
        verify: python(VERIFY, &[]),
        review: python("print('APPROVE')", &[]),
    }
}

fn calls(log: &Path) -> usize {
    std::fs::read_to_string(log).map_or(0, |s| s.lines().count())
}

fn commit_count(path: &Path) -> u32 {
    git_output(path, &["rev-list", "--count", "HEAD"])
        .unwrap()
        .parse()
        .unwrap()
}

fn story(project: &str) -> String {
    let backend = StoryStore::open().unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project: project.into(),
            title: "Delivery plan".into(),
            source: "plans/delivery.md".into(),
        })
        .unwrap();
    backend
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Write the feature file".into(),
            criteria: vec!["feature.txt says fixed".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap()
        .id
}

fn install_with(f: &Fixture, project: &str, commands: &Commands) {
    let installed = f.host.catalog(project).unwrap();
    let registry = f.host.registry();
    let context = f.host.publication_context(project).unwrap();
    install(
        &WorkflowStore::open().unwrap(),
        project,
        &installed,
        &registry,
        context.as_ref(),
        commands,
    )
    .unwrap();
}

fn start(state: &Arc<crate::state::AppState>, project: &str, story: &str, key: &str) -> String {
    let reply = neutral_action(
        state,
        project,
        NeutralAction::StartDelivery {
            story: story.into(),
            key: key.into(),
        },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let reply = serde_json::to_value(reply).unwrap();
    reply["value"]["snapshot"]["run"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Outer hang bound only; every script has its own smaller process timeout.
async fn settled(owner: &RuntimeOwner, id: &str) -> NeutralExecution {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = owner.store.neutral_execution(id).unwrap();
            if snapshot.status != NeutralStatus::Running {
                return snapshot;
            }
            changed.await;
        }
    })
    .await
    .expect("delivery run settles")
}

fn delivered(run: &NeutralExecution) -> bool {
    run.status
        == NeutralStatus::Completed {
            outcome: "delivered".into(),
        }
}

/// Call with the config directory override held, so every store is scratch.
fn setup() -> (Fixture, String) {
    let mut f = Fixture::new();
    let path = f.context.workspace_path().to_owned();
    f.context = NativeContext::capture_project(&f.state, NativeCaller::Owner, &path).unwrap();
    let project = path.to_string_lossy().into_owned();
    (f, project)
}

fn scratch_config() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
}

// Catches: delivery past a failed gate. A failed verification must run the
// implementer again with the findings, and only a passing attempt commits.
#[tokio::test]
async fn failed_verification_loops_to_correction_before_one_delivery() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    install_with(&f, &project, &commands(&log, false));
    let story = story(&project);
    let before = commit_count(&path);
    let run = settled(&f.owner, &start(&f.state, &project, &story, "first")).await;
    assert!(delivered(&run), "{:?}", run.status);
    assert_eq!(calls(&log), 2, "the failed attempt was corrected once");
    assert_eq!(commit_count(&path), before + 1);
    assert_eq!(
        git_output(&path, &["show", "HEAD:feature.txt"]).unwrap(),
        "fixed"
    );
    let runs = neutral_action(
        &f.state,
        &project,
        NeutralAction::DeliveryRuns { story },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let runs = serde_json::to_value(runs).unwrap();
    assert_eq!(runs["value"][0]["run_id"], json!(run.run.id));
    assert_eq!(runs["value"][0]["key"], json!("first"));
}

// Catches: the deliver step sweeping unrelated files (git add -A): an
// untracked file that existed before the run must stay out of the commit.
#[tokio::test]
async fn untracked_file_present_before_the_run_stays_out_of_the_delivered_commit() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    install_with(&f, &project, &commands(&log, false));
    let story = story(&project);
    std::fs::write(path.join("local-notes.txt"), "not part of the story").unwrap();
    let run = settled(&f.owner, &start(&f.state, &project, &story, "untracked")).await;
    assert!(delivered(&run), "{:?}", run.status);
    let committed = git_output(&path, &["show", "--name-only", "--format=", "HEAD"]).unwrap();
    assert_eq!(committed.trim(), "feature.txt");
    assert_eq!(
        git_output(&path, &["status", "--porcelain"])
            .unwrap()
            .trim(),
        "?? local-notes.txt"
    );
}

// Catches: delivery past a failed gate when the correction budget runs out:
// an exhausted loop must end blocked with no commit at all.
#[tokio::test]
async fn verification_that_never_passes_ends_blocked_without_a_commit() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    let mut never = commands(&log, false);
    never.verify = python("import sys\nsys.exit(1)\n", &[]);
    install_with(&f, &project, &never);
    let story = story(&project);
    let before = commit_count(&path);
    let run = settled(&f.owner, &start(&f.state, &project, &story, "never")).await;
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "blocked".into()
        }
    );
    assert_eq!(calls(&log), 3, "every attempt of the bounded loop ran");
    assert_eq!(commit_count(&path), before);
    assert!(
        run.effects
            .values()
            .all(|e| e.invocation.descriptor.id.name != "deliver")
    );
}

/// Drive one owner by hand until `stop` holds, as the actor would.
async fn drive_until(
    owner: &Arc<RuntimeOwner>,
    id: &str,
    stop: impl Fn(&NeutralExecution) -> bool,
) -> NeutralExecution {
    for _ in 0..200 {
        let (snapshot, _) = owner.drive_neutral_turn(id).unwrap();
        if stop(&snapshot) {
            return snapshot;
        }
        assert_eq!(snapshot.status, NeutralStatus::Running);
        let notify = Arc::new(Notify::new());
        assert!(
            owner.dispatch_neutral(&snapshot, &notify).unwrap(),
            "run stalled"
        );
        let after = owner.store.neutral_execution(id).unwrap();
        if after
            .effects
            .values()
            .any(|e| e.dispatched && e.observation.is_none())
        {
            notify.notified().await;
        }
    }
    panic!("stop condition not reached");
}

// Catches: double delivery after restart, and a restarted owner failing the
// next step of an unbound run instead of resuming it at its recorded node.
#[tokio::test]
async fn restarted_owner_resumes_the_run_without_repeating_a_committed_effect() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    install_with(&f, &project, &commands(&log, true));
    let story = story(&project);
    let before = commit_count(&path);
    // Start without an actor so the test owns the interleaving.
    let store = WorkflowStore::open().unwrap();
    let draft = store
        .list_neutral_drafts(&project)
        .unwrap()
        .into_iter()
        .find(|d| d.name == super::plan_delivery::NAME)
        .unwrap();
    let request: super::neutral::NeutralStartRequest = serde_json::from_value(json!({
        "publication": {"project": project, "id": draft.id, "revision": draft.latest_published_revision},
        "request_id": format!("{}{story}:resume", super::plan_delivery::RUN_PREFIX),
        "inputs": {"story": story_input(&project, &story).unwrap()},
        "subject": {"workspace": {"reference": "workspace", "id": project}},
        "limits": {"max_activations": 200, "max_concurrency": 1, "max_duration_ms": 86_400_000},
    }))
    .unwrap();
    let run = {
        let installed = f.host.catalog(&project).unwrap();
        let registry = f.host.registry();
        let catalog = store
            .build_project_catalog(&project, &installed, &registry)
            .unwrap();
        f.owner
            .start_neutral(&store, &request, &catalog, &registry.available_handlers())
            .unwrap()
    };
    f.host
        .bind_run(&f.owner.store, &run.id, f.context.clone())
        .unwrap();
    let implemented = |s: &NeutralExecution| {
        s.effects
            .values()
            .any(|e| e.invocation.descriptor.id.name == "implement" && e.observation.is_some())
    };
    drive_until(&f.owner, &run.id, implemented).await;
    assert_eq!(calls(&log), 1);

    // Restart: a new owner and a new host with no caller binding.
    let Fixture {
        dir, state, owner, ..
    } = f;
    drop(owner);
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let host = NativeHost::install(&state).unwrap();
    owner.neutral_host(host.clone());
    state.workflow_runtime.attach_owner(owner.clone());
    assert!(!host.bound(&run.id));
    // Recovery wakes every run. Unbound, it must wait rather than fail a step.
    let wake = Arc::new(Notify::new());
    let waiting = super::neutral_runtime::actor(Arc::downgrade(&owner), &run.id, &wake);
    assert!(
        tokio::time::timeout(Duration::from_millis(500), waiting)
            .await
            .is_err(),
        "the unbound run ended: {:?}",
        owner.store.neutral_execution(&run.id).unwrap().status
    );
    assert_eq!(
        owner.store.neutral_execution(&run.id).unwrap().status,
        NeutralStatus::Running
    );

    // Resume: the same story and key rebinds the caller and continues.
    assert_eq!(start(&state, &project, &story, "resume"), run.id);
    let finished = settled(&owner, &run.id).await;
    assert!(delivered(&finished), "{:?}", finished.status);
    assert_eq!(
        calls(&log),
        1,
        "the committed implement effect never repeats"
    );
    assert_eq!(commit_count(&path), before + 1);
    let delivers = finished
        .effects
        .values()
        .filter(|e| e.invocation.descriptor.id.name == "deliver")
        .count();
    assert_eq!(delivers, 1);
}
