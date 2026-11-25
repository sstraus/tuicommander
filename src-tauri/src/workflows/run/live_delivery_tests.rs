//! Live acceptance C: the plan and story delivery workflow runs with a REAL
//! agent (`ego run`) as implementer and reviewer, in a throwaway Git workspace.
//! Opt-in (`--ignored`). Two real model runs (implement + review); the wrapper
//! refuses a third, so a rejected review cannot spend the suite budget.
use super::live_support::{Live, MODEL_BUDGET};
use super::native_test_support::Fixture;
use super::neutral_execution::NeutralStatus;
use super::neutral_runtime::NeutralHost;
use super::plan_delivery::{Commands, install};
use super::{NativeCaller, NativeContext, check::git_output};
use crate::stories::{NewPlan, NewStory, StoryOrigin, StoryStore};
use crate::workflows::WorkflowStore;
use crate::workflows::neutral_api::{model::NeutralAction, neutral_action};
use crate::workflows::neutral_host::NeutralCatalogHost;
use serde_json::json;
use std::time::Duration;

const DEFAULT_MODEL: &str = "openrouter/anthropic/claude-haiku-5.5";
/// Spends one run from the shared budget file, then runs the agent with a TMPDIR
/// outside the workspace (ego refuses a writable root that contains it).
const BUDGETED_AGENT: &str = "import os,subprocess,sys\nf,mx,tmp=sys.argv[1:4]\nn=int(open(f).read().strip() or 0) if os.path.exists(f) else 0\nif n>=int(mx):\n sys.stderr.write('model budget spent');sys.exit(3)\nopen(f,'w').write(str(n+1))\nsys.exit(subprocess.run(sys.argv[4:],env=dict(os.environ,TMPDIR=tmp)).returncode)\n";
const VERIFY: &str =
    "import sys\nsys.exit(0 if open('delivered.txt').read().strip()=='delivered' else 1)\n";

fn python(script: &str, args: &[&str]) -> Vec<String> {
    let program = if cfg!(windows) { "python" } else { "python3" };
    [program, "-c", script]
        .into_iter()
        .chain(args.iter().copied())
        .map(String::from)
        .collect()
}

// Catches: a delivery graph that completes only with scripted agents: a real
// implementer that never writes the artifact, a real reviewer reply the
// APPROVE/FIX parser misreads, or a commit that misses the delivered bytes.
#[tokio::test]
#[ignore = "live: needs TUIC_LIVE_EGO, TUIC_LIVE_BUDGET_FILE and provider credentials (OPENROUTER_API_KEY)"]
async fn live_c_plan_delivery_commits_the_bytes_a_real_agent_wrote() {
    let mut live = Live::start("live_c_plan_delivery_commits_the_bytes_a_real_agent_wrote");
    if live.gate(
        &[
            "TUIC_LIVE_EGO",
            "TUIC_LIVE_BUDGET_FILE",
            "OPENROUTER_API_KEY",
        ],
        "skipped",
    ) {
        return;
    }
    let env = |name: &str| std::env::var(name).unwrap();
    let model = std::env::var("TUIC_LIVE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let ego_tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let mut f = Fixture::new();
    let path = f.context.workspace_path().to_owned();
    assert!(!path.starts_with(ego_tmp.path()));
    f.context = NativeContext::capture_project(&f.state, NativeCaller::Owner, &path).unwrap();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let project = path.to_string_lossy().into_owned();
    let (budget, max, tmp, ego) = (
        env("TUIC_LIVE_BUDGET_FILE"),
        MODEL_BUDGET.to_string(),
        ego_tmp.path().to_string_lossy().into_owned(),
        env("TUIC_LIVE_EGO"),
    );
    let agent = |sandbox: &str| {
        python(
            BUDGETED_AGENT,
            &[
                &budget,
                &max,
                &tmp,
                &ego,
                "run",
                "--sandbox",
                sandbox,
                "-m",
                &model,
                "-",
            ],
        )
    };
    let commands = Commands {
        implement: agent("workspace"),
        verify: python(VERIFY, &[]),
        review: agent("ro"),
    };
    let installed = f.host.catalog(&project).unwrap();
    let registry = f.host.registry();
    let context = f.host.publication_context(&project).unwrap();
    install(
        &WorkflowStore::open().unwrap(),
        &project,
        &installed,
        &registry,
        context.as_ref(),
        &commands,
    )
    .unwrap();
    let backend = StoryStore::open().unwrap();
    let plan = backend
        .create_plan(NewPlan {
            project: project.clone(),
            title: "Live delivery plan".into(),
            source: "plans/delivery.md".into(),
        })
        .unwrap();
    let story = backend
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Add delivered.txt".into(),
            criteria: vec![
                "the file delivered.txt exists at the repository root".into(),
                "its content is exactly one line: delivered".into(),
            ],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap()
        .id;
    let before = git_output(&path, &["rev-list", "--count", "HEAD"]).unwrap();
    let reply = neutral_action(
        &f.state,
        &project,
        NeutralAction::StartDelivery {
            story: story.clone(),
            key: "live".into(),
        },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let reply = serde_json::to_value(reply).unwrap();
    let id = reply["value"]["snapshot"]["run"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Outer hang bound only; each script has its own process timeout.
    let run = tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            let changed = f.owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = f.owner.store.neutral_execution(&id).unwrap();
            if snapshot.status != NeutralStatus::Running {
                return snapshot;
            }
            changed.await;
        }
    })
    .await
    .expect("the live delivery run settles");
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "delivered".into()
        },
        "{run:?}"
    );
    assert_ne!(
        git_output(&path, &["rev-list", "--count", "HEAD"]).unwrap(),
        before,
        "the live run committed nothing"
    );
    assert_eq!(
        git_output(&path, &["show", "HEAD:delivered.txt"])
            .unwrap()
            .trim(),
        "delivered"
    );
    live.finish(
        "passed",
        json!({"run_id": id, "model": model, "cost": "2 real model runs (implement + review)"}),
    );
}
