// Where persistent-looking fixtures live. They used to sit in `<checkout>/.tmp`; when a fixture's
// `git init` failed there (the agent sandbox denies the .git/config write), later fixture git
// calls discovered the enclosing REAL repository and ran `git checkout --detach` in it, detaching
// the developer's worktree. The root must stay outside every repository and behind the git fence.

use std::process::Command;

// Catches: the persistent fixture root moving back into (or under) a repository, where a failed
// fixture `git init` lets git discover the surrounding checkout.
#[test]
fn non_temporary_fixture_root_is_outside_every_repository() {
    let root = tuic_test_support::non_temporary_fixture_root();
    let probe = tempfile::tempdir_in(&root).unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(probe.path())
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "git found a repository above {}: {}",
        probe.path().display(),
        String::from_utf8_lossy(&output.stdout)
    );
}

// Catches: the fixture root being spelled through a symlink (`/tmp`, `/var` on macOS) so paths git
// reports physically never compare equal to the fixture's own.
#[test]
fn non_temporary_fixture_root_is_physical() {
    let root = tuic_test_support::non_temporary_fixture_root();
    assert_eq!(root, root.canonicalize().unwrap());
}
