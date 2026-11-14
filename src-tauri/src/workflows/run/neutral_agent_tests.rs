use super::check::git_output;
use super::neutral_agent::{CoarseWorkspace, WorkspaceMutation};
use std::path::Path;

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    git_output(dir.path(), &["init"]).unwrap();
    git_output(dir.path(), &["config", "user.name", "Workflow test"]).unwrap();
    git_output(
        dir.path(),
        &["config", "user.email", "workflow@example.invalid"],
    )
    .unwrap();
    std::fs::write(dir.path().join("artifact[1].txt"), "original").unwrap();
    commit(dir.path());
    dir
}

fn commit(path: &Path) {
    git_output(path, &["add", "."]).unwrap();
    git_output(path, &["-c", "core.hooksPath=", "commit", "-m", "fixture"]).unwrap();
}

// Catches completion accepting an uncommitted or later-mutated artifact claim.
#[test]
fn coarse_evidence_pins_committed_blobs_and_refuses_dirty_output() {
    let dir = repo();
    let scope = CoarseWorkspace::capture(dir.path(), WorkspaceMutation::Mutate).unwrap();
    std::fs::write(dir.path().join("artifact[1].txt"), "completed").unwrap();
    assert!(scope.pin_artifact(&["artifact[1].txt".into()]).is_err());
    commit(dir.path());
    let evidence = scope.pin_artifact(&["artifact[1].txt".into()]).unwrap();
    assert_ne!(evidence.commit, scope.baseline_commit);
    assert_eq!(
        evidence.blobs["artifact[1].txt"],
        git_output(dir.path(), &["rev-parse", "HEAD:artifact[1].txt"]).unwrap()
    );
    std::fs::write(dir.path().join("artifact[1].txt"), "after report").unwrap();
    assert!(scope.pin_artifact(&["artifact[1].txt".into()]).is_err());
    assert_eq!(
        git_output(
            dir.path(),
            &["show", &format!("{}:artifact[1].txt", evidence.commit)]
        )
        .unwrap(),
        "completed"
    );
}

// Catches treating a read-only label as evidence that the workspace did not change.
#[test]
fn observational_coarse_evidence_refuses_committed_mutation() {
    let dir = repo();
    let scope = CoarseWorkspace::capture(dir.path(), WorkspaceMutation::Observe).unwrap();
    assert!(scope.pin_artifact(&["artifact[1].txt".into()]).is_ok());
    std::fs::write(dir.path().join("artifact[1].txt"), "changed").unwrap();
    commit(dir.path());
    assert!(
        scope
            .pin_artifact(&[])
            .unwrap_err()
            .contains("changed its workspace")
    );
}

// Catches evidence escaping its workspace, matching a pathspec, or naming no real file.
#[test]
fn coarse_evidence_refuses_escaped_missing_and_duplicate_artifacts() {
    let dir = repo();
    let scope = CoarseWorkspace::capture(dir.path(), WorkspaceMutation::Mutate).unwrap();
    for path in [
        "../escape",
        "/absolute",
        "C:\\escape",
        "a/../b",
        "a\\b",
        "missing",
        "artifact*.txt",
        "./artifact[1].txt",
    ] {
        assert!(scope.pin_artifact(&[path.into()]).is_err(), "{path}");
    }
    assert!(
        scope
            .pin_artifact(&["artifact[1].txt".into(), "artifact[1].txt".into()])
            .is_err()
    );
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    assert!(CoarseWorkspace::capture(&nested, WorkspaceMutation::Mutate).is_err());
}

// Catches a committed symlink being accepted as an in-workspace content artifact.
#[cfg(unix)]
#[test]
fn coarse_evidence_refuses_committed_symlinks() {
    let dir = repo();
    let scope = CoarseWorkspace::capture(dir.path(), WorkspaceMutation::Mutate).unwrap();
    std::os::unix::fs::symlink("artifact[1].txt", dir.path().join("link")).unwrap();
    commit(dir.path());
    assert!(
        scope
            .pin_artifact(&["link".into()])
            .unwrap_err()
            .contains("regular committed file")
    );
}

#[path = "neutral_agent_adapter_tests.rs"]
mod adapter;
