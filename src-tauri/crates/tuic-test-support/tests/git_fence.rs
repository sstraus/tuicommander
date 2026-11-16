// A test process never discovers a repository above its scratch root, and its
// `git init` copies no hooks/*.sample files: the constructor installs both.

use std::path::{Path, PathBuf};
use tuic_test_support::{git_ceiling_directories, test_temp_root};

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git")
}

// Catches: a fixture whose `git init` failed falling through to the enclosing real
// checkout (the root lives inside it by default) and renaming its branches.
#[test]
fn git_under_the_test_root_does_not_discover_a_repository_above_it() {
    let root = test_temp_root();
    let probe = tempfile::tempdir_in(&root).expect("probe dir");
    let found = git(probe.path(), &["rev-parse", "--show-toplevel"]);
    assert!(
        !found.status.success(),
        "git found a repository above the test root: {}",
        String::from_utf8_lossy(&found.stdout)
    );
}

// Catches: the fence also hiding a fixture repository created inside the root.
#[test]
fn a_fixture_repository_inside_the_root_is_still_found() {
    let root = test_temp_root();
    let repo = tempfile::tempdir_in(&root).expect("fixture dir");
    assert!(git(repo.path(), &["init", "-q"]).status.success());
    std::fs::create_dir(repo.path().join("sub")).unwrap();
    let top = git(&repo.path().join("sub"), &["rev-parse", "--show-toplevel"]);
    assert!(top.status.success());
}

// Catches: git's own hooks/*.sample files being copied into every scratch repo.
#[test]
fn git_init_copies_no_template_files() {
    let repo = tempfile::tempdir_in(test_temp_root()).expect("fixture dir");
    assert!(git(repo.path(), &["init", "-q"]).status.success());
    assert!(!repo.path().join(".git/hooks/pre-commit.sample").exists());
    // ...yet the directories tests write into exist, as after a real `git init`.
    assert!(repo.path().join(".git/hooks").is_dir());
    assert!(repo.path().join(".git/info/exclude").is_file());
}

// Catches: the ceiling dropping a stricter value an outer wrapper set, or
// listing an entry twice.
#[test]
fn the_ceiling_keeps_an_inherited_value_and_lists_each_entry_once() {
    let root = PathBuf::from(if cfg!(windows) {
        r"C:\scratch\root"
    } else {
        "/scratch/root"
    });
    let sep = if cfg!(windows) { ";" } else { ":" };
    let once = git_ceiling_directories(&root, Some(std::ffi::OsStr::new("outer"))).unwrap();
    let value = once.to_string_lossy().into_owned();
    assert!(value.starts_with("outer"), "{value}");
    let parent = root.parent().unwrap().to_string_lossy().into_owned();
    assert_eq!(value.split(sep).filter(|entry| *entry == parent).count(), 1);
    let again = git_ceiling_directories(&root, Some(once.as_os_str())).unwrap();
    assert_eq!(again, once);
}

// Catches: a relative root, which git silently ignores as a ceiling entry.
#[test]
fn a_relative_root_is_refused() {
    assert!(git_ceiling_directories(Path::new("relative/root"), None).is_err());
}
