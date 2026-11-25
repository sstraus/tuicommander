//! Delivery acceptance C after the legacy conversion tests were deleted: the
//! review gate, not only the verify gate, must route back to the fix owner,
//! and a repeated start must never deliver twice. Child of `plan_delivery_tests`
//! to share its fixture; only the agent commands are local test programs.
use super::*;

/// Appends the whole prompt to the log (record separator `\x1e`), then fixes.
const RECORDING_IMPLEMENT: &str = "import sys\np=sys.stdin.read()\nopen(sys.argv[1],'a').write(p+'\\x1e')\nopen('feature.txt','w').write('fixed')\n";

fn prompts(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .split('\x1e')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect()
}

/// A reviewer that answers FIX with `finding` the first `rejections` times.
fn reviewer(state: &Path, rejections: usize, finding: &str) -> Vec<String> {
    let script = "import sys\nsys.stdin.read()\nn=int(open(sys.argv[1]).read()) if __import__('os').path.exists(sys.argv[1]) else 0\nopen(sys.argv[1],'w').write(str(n+1))\nprint(('FIX\\n'+sys.argv[3]) if n<int(sys.argv[2]) else 'APPROVE')\n";
    python(
        script,
        &[state.to_str().unwrap(), &rejections.to_string(), finding],
    )
}

fn recording(log: &Path, review: Vec<String>) -> Commands {
    Commands {
        implement: python(RECORDING_IMPLEMENT, &[log.to_str().unwrap()]),
        verify: python(VERIFY, &[]),
        review,
    }
}

// Catches: a review rejection ending the delivery (or committing anyway)
// instead of re-entering the fix owner with the reviewer's findings, and the
// findings never reaching the implementer prompt.
#[tokio::test]
async fn review_rejection_reenters_the_implementer_with_findings_before_one_commit() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    let review = reviewer(
        &f.dir.path().join("review.count"),
        1,
        "handle-the-empty-case",
    );
    install_with(&f, &project, &recording(&log, review));
    let story = story(&project);
    let before = commit_count(&path);
    let run = settled(&f.owner, &start(&f.state, &project, &story, "review")).await;
    assert!(delivered(&run), "{:?}", run.status);
    let seen = prompts(&log);
    assert_eq!(seen.len(), 2, "one correction after the rejection");
    assert!(!seen[0].contains("previous attempt was rejected"));
    assert!(
        seen[1].contains("previous attempt was rejected")
            && seen[1].contains("handle-the-empty-case"),
        "the second prompt carries the review findings: {}",
        seen[1]
    );
    assert_eq!(commit_count(&path), before + 1, "one delivery commit only");
}

// Catches: a reviewer that never approves still delivering when the budget is
// spent (review treated as advisory), or looping past the bounded attempts.
#[tokio::test]
async fn review_that_never_approves_ends_blocked_after_the_bounded_attempts_without_a_commit() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    let review = reviewer(&f.dir.path().join("review.count"), 99, "still-wrong");
    install_with(&f, &project, &recording(&log, review));
    let story = story(&project);
    let before = commit_count(&path);
    let run = settled(&f.owner, &start(&f.state, &project, &story, "never")).await;
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "blocked".into()
        }
    );
    assert_eq!(prompts(&log).len(), 3, "every bounded attempt ran, no more");
    assert_eq!(commit_count(&path), before);
    assert!(
        run.effects
            .values()
            .all(|e| e.invocation.descriptor.id.name != "deliver")
    );
}

// Catches: a repeated start with the same caller key (double click, retry
// after a lost response) creating a second run and a second delivery commit.
#[tokio::test]
async fn repeated_start_with_the_same_key_returns_the_same_run_and_commits_once() {
    let config = scratch_config();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (f, project) = setup();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let path = f.context.workspace_path().to_owned();
    let log = f.dir.path().join("implement.log");
    install_with(&f, &project, &commands(&log, true));
    let story = story(&project);
    let before = commit_count(&path);
    let first = start(&f.state, &project, &story, "same");
    let second = start(&f.state, &project, &story, "same");
    assert_eq!(first, second, "same key, same run");
    let run = settled(&f.owner, &first).await;
    assert!(delivered(&run), "{:?}", run.status);
    assert_eq!(calls(&log), 1);
    assert_eq!(commit_count(&path), before + 1);
    let other = start(&f.state, &project, &story, "other");
    assert_ne!(other, first, "a different key is a different run");
}
