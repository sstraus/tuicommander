//! Critic tests for 957-dc59 (canonical merge tree) and 953-feed (concurrent check receipts).
use super::critic_tests::{Flow, accepted_flow};
use std::path::PathBuf;

fn git(path: &std::path::Path, args: &[&str]) {
    crate::git_cli::git_cmd(path).args(args).run().unwrap();
}

fn canonical(flow: &Flow) -> PathBuf {
    PathBuf::from(flow.store.snapshot(&flow.run_id).unwrap().project)
}

fn checked(flow: &Flow) -> i64 {
    flow.store
        .execute_check(
            &flow.run_id,
            &flow.story_id,
            "repository-integrity",
            "check",
            flow.sequence,
        )
        .expect("check runs")
        .sequence
}

fn integration_error(flow: &Flow, sequence: i64) -> String {
    flow.store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", sequence)
        .expect_err("integration must be refused")
}

fn has_integration_receipt(flow: &Flow) -> bool {
    flow.store
        .snapshot(&flow.run_id)
        .unwrap()
        .stories
        .iter()
        .any(|item| item.integration_receipt.is_some())
}

#[cfg(unix)]
#[test]
fn evil_merge_that_only_flips_a_file_mode_is_rejected() {
    // catches: comparing only blob content or file names, so a mode-only change in the merge commit is certified.
    let (flow, _guard) = accepted_flow(false);
    let sequence = checked(&flow);
    let repo = canonical(&flow);
    git(&repo, &["merge", "--no-ff", "--no-commit", "story"]);
    {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let path = repo.join("story.txt");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&repo, &["add", "story.txt"]);
    git(&repo, &["commit", "-qm", "merge story"]);
    let error = integration_error(&flow, sequence);
    assert!(
        error.contains("differs from the verified clean merge"),
        "{error}"
    );
    assert!(!has_integration_receipt(&flow));
}

#[test]
fn evil_merge_that_edits_an_existing_file_is_rejected() {
    // catches: a check that only notices added files, so an edit to a pre-existing file rides along in the merge.
    let (flow, _guard) = accepted_flow(false);
    let sequence = checked(&flow);
    let repo = canonical(&flow);
    git(&repo, &["merge", "--no-ff", "--no-commit", "story"]);
    std::fs::write(repo.join("README.md"), "base\nsmuggled\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-qm", "merge story"]);
    let error = integration_error(&flow, sequence);
    assert!(
        error.contains("differs from the verified clean merge"),
        "{error}"
    );
}

#[test]
fn diverged_branches_with_a_rename_on_main_still_integrate() {
    // catches: computing the expected tree from the wrong base/parents (or without rename detection),
    // which would refuse every real merge where main advanced while the story was in review.
    let (flow, _guard) = accepted_flow(false);
    let sequence = checked(&flow);
    let repo = canonical(&flow);
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    git(&repo, &["mv", "README.md", "docs/README.md"]);
    std::fs::write(repo.join("other.txt"), "main moved on\n").unwrap();
    git(&repo, &["add", "other.txt"]);
    git(&repo, &["commit", "-qm", "main advances"]);
    let main_tip = crate::git_cli::git_cmd(&repo)
        .args(["rev-parse", "HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    git(&repo, &["merge", "--no-ff", "--no-edit", "story"]);
    let receipt = flow
        .store
        .record_integrated_story(&flow.run_id, &flow.story_id, "integrate", sequence)
        .expect("a clean diverged merge is certified");
    let stored = receipt.snapshot.stories[0]
        .integration_receipt
        .clone()
        .unwrap();
    assert_eq!(stored.base_commit, main_tip);
}

#[test]
fn conflict_resolved_with_a_strategy_option_is_rejected() {
    // catches: treating `-X ours` auto-resolution as a clean merge, certifying a silently resolved conflict.
    let (flow, _guard) = accepted_flow(false);
    let sequence = checked(&flow);
    let repo = canonical(&flow);
    std::fs::write(repo.join("story.txt"), "main version\n").unwrap();
    git(&repo, &["add", "story.txt"]);
    git(&repo, &["commit", "-qm", "main adds story.txt"]);
    git(
        &repo,
        &["merge", "--no-ff", "-X", "ours", "--no-edit", "story"],
    );
    let error = integration_error(&flow, sequence);
    assert!(error.contains("human review"), "{error}");
    assert!(!has_integration_receipt(&flow));
}

#[test]
fn concurrent_retries_of_one_check_command_record_a_single_receipt() {
    // catches: dropping the service mutex without an in-transaction command-id re-read, so two racing
    // retries of the same command id both append a CheckRecorded event or one fails as "reused".
    let (flow, _guard) = accepted_flow(false);
    let flow = &flow;
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(move || {
                    flow.store.execute_check(
                        &flow.run_id,
                        &flow.story_id,
                        "repository-integrity",
                        "same-command",
                        flow.sequence,
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let receipts: Vec<_> = results
        .into_iter()
        .map(|r| r.expect("both retries succeed"))
        .collect();
    assert_eq!(receipts[0].sequence, receipts[1].sequence);
    let snapshot = flow.store.snapshot(&flow.run_id).unwrap();
    assert_eq!(snapshot.stories[0].check_receipts.len(), 1);
    assert_eq!(snapshot.sequence, flow.sequence + 1);
}
