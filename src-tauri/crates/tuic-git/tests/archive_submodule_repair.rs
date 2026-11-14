//! Archiving a linked checkout must repair every submodule repository that lives
//! outside the checkout, and leave embedded clones (repository inside the checkout)
//! usable after the move. Real git, real filesystem, public entry points only.

use std::path::{Path, PathBuf};
use std::process::Command;

use tuic_git::worktree::{WorktreeConfig, archive_worktree, create_workspace_recycling};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "protocol.file.allow=always",
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {dir:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn module_source(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), name).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "init"]);
    dir
}

/// Base repo with submodules `names`, plus a linked worktree `feat` with all of them initialised.
fn fixture(root: &Path, names: &[&str]) -> (PathBuf, PathBuf) {
    let base = root.join("base");
    std::fs::create_dir_all(&base).unwrap();
    git(&base, &["init", "-q", "-b", "main"]);
    std::fs::write(base.join("README"), "x").unwrap();
    git(&base, &["add", "."]);
    git(&base, &["commit", "-q", "-m", "init"]);
    for name in names {
        let src = module_source(&root.join("srcs"), name);
        git(
            &base,
            &["submodule", "add", "-q", src.to_str().unwrap(), name],
        );
    }
    git(&base, &["commit", "-q", "-m", "modules"]);
    let wts = root.join("wts");
    std::fs::create_dir_all(&wts).unwrap();
    let wt = wts.join("feat");
    git(
        &base,
        &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()],
    );
    git(&wt, &["submodule", "update", "--init", "--recursive", "-q"]);
    (base, wt)
}

/// Replace the registered submodule checkout by an embedded clone (`.git` is a directory).
fn embed(root: &Path, wt: &Path, name: &str) {
    let src = root.join("srcs").join(name);
    std::fs::remove_dir_all(wt.join(name)).unwrap();
    git(wt, &["clone", "-q", src.to_str().unwrap(), name]);
    assert!(
        wt.join(name).join(".git").is_dir(),
        "fixture: embedded clone expected"
    );
}

fn assert_module_usable_at(checkout: &Path, name: &str) {
    let module = checkout.join(name);
    let top = git(&module, &["rev-parse", "--show-toplevel"]);
    assert_eq!(
        std::fs::canonicalize(&top).unwrap(),
        std::fs::canonicalize(&module).unwrap(),
        "submodule {name} resolves to a different worktree"
    );
    assert_eq!(
        git(&module, &["status", "--porcelain"]),
        "",
        "submodule {name} not clean"
    );
}

/// Catches: the embedded-clone skip also skips absorbed modules of a LINKED worktree
/// (gitdir under `.git/worktrees/<n>/modules`), leaving core.worktree pointing at the old path.
#[test]
fn archive_still_repairs_an_absorbed_submodule_of_a_linked_worktree() {
    let root = tempfile::tempdir().unwrap();
    let (base, wt) = fixture(root.path(), &["plugins"]);
    let gitdir = git(&wt.join("plugins"), &["rev-parse", "--absolute-git-dir"]);
    assert!(
        gitdir.contains("/worktrees/"),
        "fixture: module must be absorbed under the worktree admin dir: {gitdir}"
    );

    let archived = PathBuf::from(archive_worktree(&base, "feat", None).expect("archive"));

    assert!(!wt.exists());
    assert_module_usable_at(&archived, "plugins");
    let configured = git(
        &archived,
        &[
            "config",
            "--file",
            &format!("{gitdir}/config"),
            "core.worktree",
        ],
    );
    assert_eq!(
        std::fs::canonicalize(configured).unwrap(),
        std::fs::canonicalize(archived.join("plugins")).unwrap()
    );
}

/// Catches: a checkout holding BOTH an embedded clone and an absorbed module aborts, skips
/// the absorbed one, or repairs the embedded one into a broken state.
#[test]
fn archive_mixed_embedded_and_absorbed_submodules_leaves_both_usable() {
    let root = tempfile::tempdir().unwrap();
    let (base, wt) = fixture(root.path(), &["plugins", "vendor"]);
    embed(root.path(), &wt, "vendor");

    let archived = PathBuf::from(archive_worktree(&base, "feat", None).expect("archive"));

    assert_module_usable_at(&archived, "plugins");
    assert_module_usable_at(&archived, "vendor");
    assert!(
        archived.join("vendor/.git").is_dir(),
        "embedded clone must stay a directory repo"
    );
}

/// Catches: the base repository reached through a symlink makes the canonical checkout path and
/// git's own spelling disagree, so an absorbed module is treated as embedded (or the archive aborts).
#[cfg(unix)]
#[test]
fn archive_through_a_symlinked_base_path_still_repairs_absorbed_modules() {
    let root = tempfile::tempdir().unwrap();
    let (base, wt) = fixture(root.path(), &["plugins"]);
    let link = root.path().join("link-base");
    std::os::unix::fs::symlink(&base, &link).unwrap();
    let _ = wt;

    let archived = PathBuf::from(archive_worktree(&link, "feat", None).expect("archive"));

    assert_module_usable_at(&archived, "plugins");
}

/// Catches: restore (archive -> original slot, `move_registered_checkout` in reverse) leaves an
/// absorbed module pointing at the archive path, or breaks an embedded clone.
#[test]
fn restore_after_archive_keeps_absorbed_and_embedded_submodules_usable() {
    let root = tempfile::tempdir().unwrap();
    let (base, wt) = fixture(root.path(), &["plugins", "vendor"]);
    embed(root.path(), &wt, "vendor");
    let wts = wt.parent().unwrap().to_path_buf();
    archive_worktree(&base, "feat", None).expect("archive");
    assert!(!wt.exists());

    let config = WorktreeConfig {
        task_name: "feat".into(),
        base_repo: base.to_string_lossy().into_owned(),
        branch: Some("feat".into()),
        create_branch: true,
    };
    let (restored, _) = create_workspace_recycling(&wts, &config, None, |_| true)
        .expect("restore archived checkout");

    assert_eq!(
        std::fs::canonicalize(&restored.path).unwrap(),
        std::fs::canonicalize(&wt).unwrap()
    );
    assert_module_usable_at(&wt, "plugins");
    assert_module_usable_at(&wt, "vendor");
}

/// Catches: an archive of an embedded-only checkout still fails (the reported production error),
/// or leaves the registration unrepaired so `git worktree list` shows a prunable entry.
#[test]
fn archive_embedded_only_checkout_registers_the_new_path() {
    let root = tempfile::tempdir().unwrap();
    let (base, wt) = fixture(root.path(), &["plugins"]);
    embed(root.path(), &wt, "plugins");

    let archived = PathBuf::from(archive_worktree(&base, "feat", None).expect("archive"));

    let listed = git(&base, &["worktree", "list", "--porcelain"]);
    let archived_canon = std::fs::canonicalize(&archived).unwrap();
    assert!(
        listed
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .any(|p| std::fs::canonicalize(p).ok().as_deref() == Some(archived_canon.as_path())),
        "archived path not registered: {listed}"
    );
    assert!(
        !listed.contains("prunable"),
        "registration left stale: {listed}"
    );
    assert_module_usable_at(&archived, "plugins");
}
