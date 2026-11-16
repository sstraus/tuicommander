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

// Catches: an inherited GIT_DIR/GIT_WORK_TREE (or friends) selecting the developer's
// repository: the ceiling never restricts an explicit GIT_DIR. The probe runs in a
// child of this binary started WITH those variables; its constructor must clear them.
#[test]
fn git_location_overrides_are_cleared_at_startup() {
    if std::env::var_os("TUIC_FENCE_CHILD").is_none() {
        let exe = std::env::current_exe().expect("current exe");
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--exact", "git_location_overrides_are_cleared_at_startup"])
            .env("TUIC_FENCE_CHILD", "1");
        for key in tuic_test_support::GIT_LOCATION_OVERRIDES {
            cmd.env(key, "/nonexistent/tuic-fence");
        }
        let out = cmd.output().expect("run the child test binary");
        assert!(
            out.status.success(),
            "child failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    for key in tuic_test_support::GIT_LOCATION_OVERRIDES {
        assert!(
            std::env::var_os(key).is_none(),
            "{key} survived the constructor"
        );
    }
}

// Catches: an inherited GIT_TEMPLATE_DIR winning over the empty template, so the
// suite depends on a developer's personal template.
#[test]
fn an_inherited_template_dir_is_replaced() {
    if std::env::var_os("TUIC_FENCE_CHILD").is_none() {
        let personal = tempfile::tempdir_in(test_temp_root()).expect("personal template");
        std::fs::create_dir_all(personal.path().join("hooks")).unwrap();
        std::fs::write(
            personal.path().join("hooks/pre-commit.sample"),
            "#!/bin/sh\n",
        )
        .unwrap();
        let out = std::process::Command::new(std::env::current_exe().expect("current exe"))
            .args(["--exact", "an_inherited_template_dir_is_replaced"])
            .env("TUIC_FENCE_CHILD", "1")
            .env("GIT_TEMPLATE_DIR", personal.path())
            .output()
            .expect("run the child test binary");
        assert!(
            out.status.success(),
            "child failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    let repo = tempfile::tempdir_in(test_temp_root()).expect("fixture dir");
    assert!(git(repo.path(), &["init", "-q"]).status.success());
    assert!(!repo.path().join(".git/hooks/pre-commit.sample").exists());
}

// Catches: lexical `..` removal naming a different directory than the OS does when a
// symlink precedes it, which fences the wrong parent.
#[cfg(unix)]
#[test]
fn a_symlink_followed_by_dotdot_fences_the_real_parent() {
    let base = tempfile::tempdir_in(test_temp_root()).expect("base dir");
    let base = std::fs::canonicalize(base.path()).unwrap();
    std::fs::create_dir_all(base.join("real/a/b")).unwrap();
    std::fs::create_dir_all(base.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(base.join("real/a/b"), base.join("elsewhere/link")).unwrap();
    // `elsewhere/link/..` is `real/a` on disk but `elsewhere` lexically.
    let root = base.join("elsewhere/link/../scratch");
    std::fs::create_dir_all(&root).unwrap();
    assert!(base.join("real/a/scratch").is_dir());
    let ceiling = git_ceiling_directories(&root, None).unwrap();
    let entries: Vec<PathBuf> = std::env::split_paths(&ceiling).collect();
    assert!(entries.contains(&base.join("real/a")), "{entries:?}");
}

// Catches: a lossy UTF-8 round trip rewriting a non-UTF-8 inherited value or entry.
#[cfg(unix)]
#[test]
fn non_utf8_ceiling_entries_are_kept_byte_for_byte() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let existing = std::ffi::OsString::from_vec(b"/outer-\xff".to_vec());
    let root = PathBuf::from(std::ffi::OsString::from_vec(b"/scratch-\xfe/root".to_vec()));
    let ceiling = git_ceiling_directories(&root, Some(existing.as_os_str())).unwrap();
    assert_eq!(ceiling.as_bytes(), b"/outer-\xff:/scratch-\xfe");
}
