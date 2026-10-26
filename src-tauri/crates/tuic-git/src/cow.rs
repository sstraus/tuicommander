//! Copy-on-write helpers used to warm linked worktrees.
//!
//! TUICommander no longer creates independent copy-on-write workspace clones.
//! It still clonefiles git-ignored build directories into a freshly created
//! linked worktree so dependency and compiler caches arrive warm.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git_cli::git_cmd;

/// The copy-on-write flags supported by macOS `cp` and GNU coreutils `cp`.
/// Every flag must fail instead of degrading to a byte copy.
const COW_COPY_FLAGS: [&str; 2] = ["-c", "--reflink=always"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CowSupport {
    Supported,
    Unsupported(String),
}

/// Probe the actual source/destination pair instead of inferring reflink
/// support from a filesystem name.
pub fn probe_cow_support(src: &Path, dest_parent: &Path) -> CowSupport {
    let probe_source = head_file(src);
    if !probe_source.is_file() {
        return CowSupport::Unsupported(format!(
            "no HEAD to probe with at '{}'",
            probe_source.display()
        ));
    }
    let Some(anchor) = existing_ancestor(dest_parent) else {
        return CowSupport::Unsupported(format!(
            "no existing directory above '{}' to probe",
            dest_parent.display()
        ));
    };
    if same_volume(src, &anchor) == Some(false) {
        return CowSupport::Unsupported(format!(
            "'{}' and '{}' are on different volumes",
            src.display(),
            anchor.display()
        ));
    }

    let probe = anchor.join(format!(
        ".tuic-cow-probe.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_file(&probe);
    let copied = COW_COPY_FLAGS
        .iter()
        .any(|flag| clone_file_with(&[flag], &probe_source, &probe));
    let _ = std::fs::remove_file(&probe);

    if copied {
        CowSupport::Supported
    } else {
        CowSupport::Unsupported(format!(
            "copy-on-write is unavailable between '{}' and '{}'",
            probe_source.display(),
            anchor.display()
        ))
    }
}

/// The HEAD file of `src`'s repository. In a linked worktree `.git` is a
/// `gitdir: <path>` pointer file, not a directory.
fn head_file(src: &Path) -> PathBuf {
    let dot_git = src.join(".git");
    if dot_git.is_file()
        && let Ok(pointer) = std::fs::read_to_string(&dot_git)
        && let Some(git_dir) = pointer.trim().strip_prefix("gitdir:")
    {
        return src.join(git_dir.trim()).join("HEAD");
    }
    dot_git.join("HEAD")
}

fn clone_file_with(flags: &[&str], from: &Path, to: &Path) -> bool {
    Command::new("cp")
        .args(flags)
        .arg(from)
        .arg(to)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
        && to.exists()
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(directory) = current {
        if directory.is_dir() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

#[cfg(unix)]
fn same_volume(left: &Path, right: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    Some(std::fs::metadata(left).ok()?.dev() == std::fs::metadata(right).ok()?.dev())
}

#[cfg(not(unix))]
fn same_volume(_left: &Path, _right: &Path) -> Option<bool> {
    None
}

/// Copy a directory tree copy-on-write, trying the platform-specific flags in
/// order and never falling back to a byte copy.
pub fn clone_tree(src: &Path, dest: &Path) -> Result<(), String> {
    clone_tree_with(src, dest, |flag| {
        let mut command = Command::new("cp");
        command.arg(flag).arg("-R").arg(src).arg(dest);
        let (status, stderr) = run_copy_command(&mut command, COW_COPY_TIMEOUT)?;
        if status.success() {
            Ok(())
        } else {
            Err(stderr.trim().to_string())
        }
    })
}

const COW_COPY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(280);

/// Run a copy command under `timeout`, returning its status and stderr.
///
/// Delegates to [`crate::git_cli::output_with_deadline`] rather than polling
/// `try_wait` and reading stderr afterwards. That ordering deadlocks: `cp`
/// prints one line per unreadable file, and once it has written a pipe buffer's
/// worth (64 KiB) with nobody draining the other end it blocks in `write` and
/// can never exit, so the child sat there until the deadline killed it — a
/// wedge, reported as a timeout. `output_with_deadline` drains both pipes on
/// reader threads, so a chatty copy finishes and reports its real error.
fn run_copy_command(
    command: &mut Command,
    timeout: std::time::Duration,
) -> Result<(std::process::ExitStatus, String), String> {
    let output = crate::git_cli::output_with_deadline(command, timeout).map_err(|error| {
        match error {
            // Keep the copy-specific wording: GitError's own Display says "git".
            crate::git_cli::GitError::TimedOut { after } => format!(
                "copy-on-write copy exceeded its {} second operation deadline",
                after.as_secs()
            ),
            other => format!("could not run copy command: {other}"),
        }
    })?;
    Ok((
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn clone_tree_with(
    src: &Path,
    dest: &Path,
    mut attempt: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for flag in COW_COPY_FLAGS {
        match attempt(flag) {
            Ok(()) => return Ok(()),
            Err(reason) => failures.push(format!("`cp {flag} -R`: {reason}")),
        }
        let _ = std::fs::remove_dir_all(dest);
    }
    Err(format!(
        "copy-on-write copy of '{}' into '{}' failed: {}",
        src.display(),
        dest.display(),
        failures.join("; ")
    ))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WarmingReport {
    pub warmed: usize,
    pub warnings: Vec<String>,
}

/// Ask git which directories are ignored. A git failure is a warning rather
/// than an error: the worktree is complete and valid, it just starts cold.
fn warming_candidates(src: &Path, dest: &Path) -> Result<Vec<PathBuf>, WarmingReport> {
    let mut candidates = ignored_directories(src).map_err(|reason| WarmingReport {
        warmed: 0,
        warnings: vec![format!(
            "could not ask git which directories are ignored, so '{}' starts cold: {reason}",
            dest.display()
        )],
    })?;
    candidates.retain(|candidate| {
        !candidate
            .components()
            .any(|component| component.as_os_str() == ".tmp" || component.as_os_str() == ".mdkb")
    });
    candidates.extend(external_bin_candidates(src));
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

fn external_bin_candidates(src: &Path) -> Vec<PathBuf> {
    let Ok(contents) = std::fs::read_to_string(src.join("src-tauri/tauri.conf.json")) else {
        return Vec::new();
    };
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return Vec::new();
    };
    let Some(entries) = config
        .pointer("/bundle/externalBin")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(serde_json::Value::as_str)
        .flat_map(|entry| {
            let declared = Path::new(entry);
            let (Some(name), Some(parent)) = (declared.file_name(), declared.parent()) else {
                return Vec::new();
            };
            let prefix = format!("{}-", name.to_string_lossy());
            std::fs::read_dir(src.join("src-tauri").join(parent))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter_map(|child| {
                    (child.path().is_file()
                        && child.file_name().to_string_lossy().starts_with(&prefix))
                    .then(|| child.path().strip_prefix(src).ok().map(PathBuf::from))
                    .flatten()
                })
                .collect()
        })
        .collect()
}

/// Production wrapper. Probe once so a filesystem without clonefile support
/// produces one useful warning instead of one failure for every ignored tree.
pub fn warm_worktree(src: &Path, dest: &Path) -> WarmingReport {
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    if candidates.is_empty() {
        // Nothing to warm: skip the probe so a non-clonefile filesystem does
        // not warn about a copy that was never going to happen.
        return WarmingReport::default();
    }
    match probe_cow_support(src, dest.parent().unwrap_or(dest)) {
        CowSupport::Supported => warm_candidates(src, dest, candidates, clone_tree),
        CowSupport::Unsupported(reason) => WarmingReport {
            warmed: 0,
            warnings: vec![format!(
                "could not warm '{}' with copy-on-write data: {reason}",
                dest.display()
            )],
        },
    }
}

/// Copy the parent's git-ignored directories into a linked worktree with an
/// injected copy primitive. A failed copy leaves a complete, valid, but cold
/// worktree and is therefore a warning.
///
/// Test-only. Production always clonefiles, and the probe that decides whether
/// it can belongs to `warm_worktree`; this seam exists so the tests can drive
/// `warm_candidates` with a copy that does not depend on the filesystem.
#[cfg(test)]
pub fn warm_worktree_with(
    src: &Path,
    dest: &Path,
    copy: impl Fn(&Path, &Path) -> Result<(), String>,
) -> WarmingReport {
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    warm_candidates(src, dest, candidates, copy)
}

fn warm_candidates(
    src: &Path,
    dest: &Path,
    candidates: Vec<PathBuf>,
    copy: impl Fn(&Path, &Path) -> Result<(), String>,
) -> WarmingReport {
    let mut report = WarmingReport::default();
    let src_root = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let dest_root = std::fs::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf());

    for relative in candidates {
        let from = src_root.join(&relative);
        let to = dest_root.join(&relative);
        match std::fs::symlink_metadata(&from) {
            Ok(metadata) if metadata.is_dir() || metadata.is_file() => {}
            _ => continue,
        }
        if from.join(".git").exists()
            || is_inside(&dest_root, &from)
            || is_inside(&from, &dest_root)
            || to.exists()
        {
            continue;
        }
        if let Some(parent) = to.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            report.warnings.push(format!(
                "could not prepare '{}' in the new worktree, which starts cold there: {error}",
                relative.display()
            ));
            continue;
        }
        match copy(&from, &to).and_then(|()| restore_owner_write(&to)) {
            Ok(()) => report.warmed += 1,
            Err(reason) => report.warnings.push(format!(
                "could not warm '{}' in the new worktree, which starts cold there: {reason}",
                relative.display()
            )),
        }
    }
    report
}

/// A clonefile preserves mode bits. Sealed build evidence is useful in the
/// new checkout, but its copied directories must remain removable by Git.
/// Repair directory write/search access before descending, without following symlinks.
pub(crate) fn restore_owner_write(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Directories need owner search before child metadata can be read.
        // Do not grant read: an unreadable tree must still fail before Git removes it.
        let required = if metadata.is_dir() { 0o300 } else { 0o200 };
        let needs_access = permissions.mode() & required != required;
        permissions.set_mode(permissions.mode() | required);
        if needs_access {
            std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
        }
    }
    #[cfg(windows)]
    {
        if permissions.readonly() {
            // Windows readonly is a file attribute; clearing it does not grant Unix world-write.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
        }
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            restore_owner_write(&entry.path())?;
        }
    }
    Ok(())
}

fn ignored_directories(src: &Path) -> Result<Vec<PathBuf>, String> {
    let listed = git_cmd(src)
        .args([
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ])
        .run()
        .map_err(|error| format!("git ls-files failed: {error}"))?;
    let mut directories: Vec<&str> = listed
        .stdout
        .split('\0')
        .filter_map(|entry| entry.strip_suffix('/'))
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            !Path::new(entry)
                .components()
                .any(|component| component.as_os_str() == ".git")
        })
        .collect();
    directories.sort_unstable();
    let mut kept: Vec<PathBuf> = Vec::new();
    for directory in directories {
        if kept
            .iter()
            .any(|ancestor| Path::new(directory).starts_with(ancestor))
        {
            continue;
        }
        kept.push(PathBuf::from(directory));
    }
    Ok(kept)
}

const WARM_ARTIFACT_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "src-tauri/target",
    ".venv",
    "venv",
    "dist",
    ".next",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WarmArtifact {
    pub path: String,
    pub size: String,
}

pub fn warm_artifacts(workspace: &Path) -> Vec<WarmArtifact> {
    let present: Vec<&str> = WARM_ARTIFACT_DIRS
        .iter()
        .copied()
        .filter(|directory| workspace.join(directory).is_dir())
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = present
            .iter()
            .map(|directory| {
                let full = workspace.join(directory);
                scope.spawn(move || directory_size(&full))
            })
            .collect();
        present
            .iter()
            .zip(handles)
            .filter_map(|(directory, handle)| {
                handle.join().ok().flatten().map(|size| WarmArtifact {
                    path: (*directory).to_string(),
                    size,
                })
            })
            .collect()
    })
}

/// Recursive size of `path`, rendered the way `du -sh` renders it.
///
/// Walks in Rust rather than shelling out to `du`. That tool does not exist on
/// Windows, where `Command::output` therefore failed and dropped every warmed
/// directory out of the report: a copy that had actually worked came back
/// saying nothing was warmed. It also spares one process per directory on the
/// platforms that do have it.
///
// DEFERRED (2026-09-14) — the other half of the portability gap. `clone_tree`
// still shells out to `cp -c` / `cp --reflink=always`, which Windows has no
// equivalent for at all. Deciding what warming even means without reflink is a
// design question, not a substitution, so it needs Boss before implementation.
fn directory_size(path: &Path) -> Option<String> {
    Some(human_size(walk_size(path)?))
}

/// Apparent bytes under `path` — file lengths, not allocated blocks, so this
/// reads slightly under a bare `du -sh` and matches `du -sh --apparent-size`.
/// The number exists to say "a warm cache this big arrived", and for that the
/// content size is the more honest of the two.
///
/// Symlinks are counted but never followed, the same choice `du` makes by
/// default: it keeps a link back into the parent repo from counting that whole
/// tree, and a cycle from never terminating.
fn walk_size(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_symlink() {
        return Some(0);
    }
    if !meta.is_dir() {
        return Some(meta.len());
    }
    let mut total = 0;
    for entry in std::fs::read_dir(path).ok()? {
        let Ok(entry) = entry else { continue };
        total += walk_size(&entry.path()).unwrap_or(0);
    }
    Some(total)
}

/// The largest unit that keeps the number under 1024, with one decimal below
/// ten — `du -sh`'s rendering, because this string is read next to sizes the
/// user has seen from `du` and a second convention would only invite comparison
/// of two numbers that do not mean the same thing.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes}B"),
        _ if value < 10.0 => format!("{value:.1}{}", UNITS[unit]),
        _ => format!("{value:.0}{}", UNITS[unit]),
    }
}

fn is_inside(inner: &Path, outer: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(inner).starts_with(canonical(outer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        git_cmd(path).args(["init"]).run().unwrap();
        std::fs::write(path.join("tracked.txt"), "tracked\n").unwrap();
        git_cmd(path).args(["add", "."]).run().unwrap();
        git_cmd(path)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ])
            .run()
            .unwrap();
    }

    #[test]
    fn probe_uses_a_real_copy_and_leaves_no_debris() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let before = std::fs::read_dir(temp.path()).unwrap().count();
        let support = probe_cow_support(&repo, &temp.path().join("missing/dest"));
        assert_eq!(before, std::fs::read_dir(temp.path()).unwrap().count());
        assert!(matches!(
            support,
            CowSupport::Supported | CowSupport::Unsupported(_)
        ));
    }

    #[test]
    fn clone_tree_falls_back_and_cleans_partial_destinations() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        let mut attempts = Vec::new();
        clone_tree_with(&src, &dest, |flag| {
            attempts.push(flag.to_string());
            if attempts.len() == 1 {
                std::fs::create_dir_all(&dest).unwrap();
                Err("unsupported".into())
            } else {
                assert!(!dest.exists());
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, vec!["-c", "--reflink=always"]);
    }

    #[test]
    fn clone_tree_exhaustion_names_each_flag_and_leaves_nothing() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        let error = clone_tree_with(&src, &dest, |flag| {
            std::fs::create_dir_all(&dest).unwrap();
            Err(format!("{flag} failed"))
        })
        .unwrap_err();
        assert!(error.contains("cp -c -R"));
        assert!(error.contains("cp --reflink=always -R"));
        assert!(!dest.exists());
    }

    /// A copy that prints more than a pipe buffer's worth of warnings must still
    /// finish and report its own exit status. `cp` emits one line per unreadable
    /// file, and reading stderr only after the child has exited inverts the
    /// dependency: once the child has written 64 KiB with nobody draining the
    /// other end it blocks in `write` and can never exit, so a noisy copy wedged
    /// until the deadline killed it and reported a timeout that never happened.
    /// The deadline here is deliberately short — a regression fails in seconds
    /// instead of hanging for the real 280.
    #[test]
    fn a_copy_that_floods_stderr_still_reports_its_own_failure() {
        // The flood has to be spelled for the host shell. `sh` is not a Windows
        // program: this test spawned it anyway and only passed on GitHub's
        // runner, whose image carries `C:\Program Files\Git\usr\bin` on `PATH`.
        let (shell, flag) = tuic_test_support::host_shell();
        let script = if cfg!(windows) {
            // What has to exceed the pipe buffer is a byte count, not a line
            // count, and `cmd`'s `for /L` costs about 14ms an iteration on a
            // small machine: measured 2026-09-16, 4000 short lines took 58s
            // and blew the 20s deadline below, while 200 long ones take 3s.
            // 200 lines of 342 bytes is 68,400 — past 64 KiB with room.
            let line = format!("cp: cannot read file{}", "x".repeat(320));
            format!("(for /L %i in (1,1,200) do @echo {line}) 1>&2 & exit /b 1")
        } else {
            "i=0; while [ $i -lt 4000 ]; do echo 'cp: cannot read file' >&2; i=$((i+1)); done; exit 1"
                .to_string()
        };
        let mut command = Command::new(shell);
        command.arg(flag).arg(script);

        let (status, stderr) =
            run_copy_command(&mut command, std::time::Duration::from_secs(20)).unwrap();

        assert_eq!(status.code(), Some(1));
        assert!(
            stderr.len() > 64 * 1024,
            "the flood must exceed a pipe buffer, got {} bytes",
            stderr.len()
        );
    }

    /// The report is the only thing telling the user a warm cache arrived, so a
    /// size it cannot measure removes the directory from the list entirely. On
    /// Windows `du` is absent and that happened to every directory, every time.
    #[test]
    fn a_directory_is_measured_without_leaving_the_process() {
        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("cache/deep");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("blob"), vec![0u8; 3000]).unwrap();
        std::fs::write(temp.path().join("cache/small"), vec![0u8; 1096]).unwrap();

        assert_eq!(
            directory_size(&temp.path().join("cache")),
            Some("4.0K".to_string())
        );
        assert_eq!(directory_size(&temp.path().join("missing")), None);
    }

    /// Read next to sizes the user has seen from `du`, so it must round the way
    /// `du -sh` rounds rather than merely being close.
    #[test]
    fn sizes_are_rendered_the_way_du_renders_them() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(1023), "1023B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(10 * 1024), "10K");
        assert_eq!(human_size(11 * 1024 * 1024), "11M");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024 / 2), "1.5G");
    }

    /// Kills 484:39 `<` to `<=` and 484:35 `+` to `*` in human_size: either
    /// lets the unit index reach 5 and panics past a pebibyte.
    #[test]
    fn a_size_beyond_the_largest_unit_stays_in_terabytes() {
        assert_eq!(human_size(1 << 50), "1024T");
    }

    /// Kills 465:8 delete `!` in walk_size: a file would be walked as a
    /// directory (size lost) and a directory would report its own inode size.
    /// 5000 is no directory's inode size, unlike 4096.
    #[test]
    fn walk_size_sums_file_lengths_not_directory_entries() {
        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("cache/deep");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("blob"), vec![0u8; 3000]).unwrap();
        std::fs::write(temp.path().join("cache/small"), vec![0u8; 2000]).unwrap();

        assert_eq!(walk_size(&temp.path().join("cache")), Some(5000));
        assert_eq!(walk_size(&temp.path().join("cache/small")), Some(2000));
    }

    /// Kills 410:5 `vec![]` in warm_artifacts, and shows that absent
    /// directories are left out.
    #[test]
    fn warm_artifacts_lists_present_directories_with_their_size() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        std::fs::write(temp.path().join("node_modules/pkg"), vec![0u8; 2048]).unwrap();
        std::fs::create_dir_all(temp.path().join("dist")).unwrap();
        std::fs::write(temp.path().join("dist/out"), vec![0u8; 10]).unwrap();

        assert_eq!(
            warm_artifacts(temp.path()),
            vec![
                WarmArtifact {
                    path: "node_modules".into(),
                    size: "2.0K".into()
                },
                WarmArtifact {
                    path: "dist".into(),
                    size: "10B".into()
                },
            ]
        );
    }

    /// Kills 26:8 delete `!` in probe_cow_support: a repo with a HEAD would be
    /// refused for having none, and a bare directory would be probed.
    #[test]
    fn probe_refuses_only_a_source_without_head() {
        let temp = TempDir::new().unwrap();
        let bare = temp.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);

        assert!(matches!(
            probe_cow_support(&bare, temp.path()),
            CowSupport::Unsupported(reason) if reason.contains("no HEAD to probe")
        ));
        assert!(!matches!(
            probe_cow_support(&repo, temp.path()),
            CowSupport::Unsupported(reason) if reason.contains("no HEAD to probe")
        ));
    }

    /// Story 1398-42b4: a source that is itself a linked worktree has a `.git`
    /// pointer file, and the probe refused it for having no HEAD.
    #[test]
    fn probe_finds_head_of_a_linked_worktree_source() {
        let (_temp, _repo, worktree) = warming_fixture();

        assert!(!matches!(
            probe_cow_support(&worktree, worktree.parent().unwrap()),
            CowSupport::Unsupported(reason) if reason.contains("no HEAD to probe")
        ));
    }

    /// Kills 38:34 `==` to `!=` in probe_cow_support, and 83:5 / 95:5 constant
    /// returns: a same-volume pair must never be reported as crossing volumes,
    /// and an existing anchor must be found.
    #[test]
    fn probe_on_one_volume_is_never_refused_for_volumes_or_anchor() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);

        match probe_cow_support(&repo, &temp.path().join("missing/dest")) {
            CowSupport::Supported => {}
            CowSupport::Unsupported(reason) => {
                assert!(!reason.contains("different volumes"), "{reason}");
                assert!(!reason.contains("no existing directory"), "{reason}");
            }
        }
    }

    /// Kills 83:5 `None` and `Some(Default::default())`.
    #[test]
    fn existing_ancestor_is_the_nearest_directory_that_exists() {
        let temp = TempDir::new().unwrap();
        assert_eq!(
            existing_ancestor(&temp.path().join("missing/deeper")),
            Some(temp.path().to_path_buf())
        );
        assert_eq!(
            existing_ancestor(temp.path()),
            Some(temp.path().to_path_buf())
        );
    }

    /// Kills 95:5 `None` / `Some(true)` / `Some(false)` and 96:46 `==` to `!=`.
    /// The crossing-volumes case needs a second filesystem and is not covered.
    #[cfg(unix)]
    #[test]
    fn same_volume_compares_devices_and_gives_up_on_a_missing_path() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("a")).unwrap();
        std::fs::create_dir_all(temp.path().join("b")).unwrap();

        assert_eq!(
            same_volume(&temp.path().join("a"), &temp.path().join("b")),
            Some(true)
        );
        assert_eq!(
            same_volume(&temp.path().join("a"), &temp.path().join("missing")),
            None
        );
    }

    /// Kills 72:5 `true` / `false` and 79:9 `&&` to `||` in clone_file_with: a
    /// failed copy must not count as success because a file already sits at the
    /// destination.
    #[cfg(unix)]
    #[test]
    fn clone_file_with_reports_whether_the_copy_happened() {
        let temp = TempDir::new().unwrap();
        let from = temp.path().join("from");
        let to = temp.path().join("to");
        let stale = temp.path().join("stale");
        std::fs::write(&from, "x").unwrap();
        std::fs::write(&stale, "old").unwrap();

        assert!(clone_file_with(&[], &from, &to));
        assert!(!clone_file_with(
            &[],
            &temp.path().join("missing"),
            &temp.path().join("none")
        ));
        assert!(!clone_file_with(&[], &temp.path().join("missing"), &stale));
    }

    /// Kills 107:5 `Ok(())` in clone_tree: a copy of a source that does not
    /// exist fails with every flag, on any filesystem.
    #[test]
    fn clone_tree_reports_a_copy_that_could_not_happen() {
        let temp = TempDir::new().unwrap();
        let error =
            clone_tree(&temp.path().join("missing"), &temp.path().join("dest")).unwrap_err();
        assert!(error.contains("copy-on-write copy of"), "{error}");
    }

    /// Kills 238:5 `Default::default()` in warm_worktree. Whether the host
    /// clones is the filesystem's call, so either outcome is accepted, but an
    /// empty report is neither.
    #[test]
    fn warm_worktree_reports_either_the_warm_copy_or_why_not() {
        let (_temp, repo, worktree) = warming_fixture();

        let report = warm_worktree(&repo, &worktree);

        if report.warmed == 1 {
            assert!(report.warnings.is_empty(), "{report:?}");
        } else {
            assert_eq!(report.warmed, 0, "{report:?}");
            assert_eq!(report.warnings.len(), 1, "{report:?}");
            assert!(report.warnings[0].contains("could not warm"), "{report:?}");
        }
    }

    /// Kills 226:25 `&&` to `||` in external_bin_candidates: a file without the
    /// sidecar prefix, or a directory that has it, is not a sidecar.
    #[test]
    fn only_files_carrying_the_sidecar_prefix_are_candidates() {
        let temp = TempDir::new().unwrap();
        let binaries = temp.path().join("src-tauri/binaries");
        std::fs::create_dir_all(binaries.join("bridge-dir")).unwrap();
        std::fs::write(binaries.join("bridge-x86"), "sidecar").unwrap();
        std::fs::write(binaries.join("other-x86"), "unrelated").unwrap();
        std::fs::write(
            temp.path().join("src-tauri/tauri.conf.json"),
            r#"{"bundle":{"externalBin":["binaries/bridge"]}}"#,
        )
        .unwrap();

        assert_eq!(
            external_bin_candidates(temp.path()),
            vec![PathBuf::from("src-tauri/binaries/bridge-x86")]
        );
    }

    /// Kills 293:29 guard `true` in warm_candidates: a symlink candidate (a
    /// sidecar that is a link) is neither file nor directory and is not copied.
    #[cfg(unix)]
    #[test]
    fn warming_does_not_copy_a_symlink() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("real"), "x").unwrap();
        std::os::unix::fs::symlink(src.join("real"), src.join("link")).unwrap();

        let report = warm_candidates(&src, &dest, vec![PathBuf::from("link")], |_, _| {
            panic!("a symlink must not be copied")
        });

        assert_eq!(report, WarmingReport::default());
    }

    fn warming_fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::write(repo.join(".gitignore"), "build/\n.env\n").unwrap();
        std::fs::create_dir_all(repo.join("build/cache")).unwrap();
        std::fs::write(repo.join("build/cache/data"), "warm").unwrap();
        std::fs::write(repo.join(".env"), "secret").unwrap();
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        (temp, repo, worktree)
    }

    fn plain_copy(from: &Path, to: &Path) -> Result<(), String> {
        if from.is_file() {
            std::fs::copy(from, to).map_err(|error| error.to_string())?;
            return Ok(());
        }
        std::fs::create_dir_all(to).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir(from).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                plain_copy(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    #[test]
    fn warming_brings_an_ignored_directory_into_the_worktree() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, plain_copy);
        assert_eq!(report.warmed, 1);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "warm"
        );
    }

    #[cfg(unix)]
    #[test]
    fn warming_unseals_only_the_cloned_evidence_tree() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join(".gitignore"), "build/\ntarget/\n.env\n").unwrap();
        let source_dir = repo.join("target/macos-system-profiler-exact20/run");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("evidence.txt");
        std::fs::write(&source_file, "sealed evidence\n").unwrap();
        std::fs::set_permissions(&source_file, std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(&source_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, to| {
            plain_copy(from, to)?;
            if from.file_name().is_some_and(|name| name == "target") {
                let cloned_dir = to.join("macos-system-profiler-exact20/run");
                std::fs::set_permissions(
                    cloned_dir.join("evidence.txt"),
                    std::fs::Permissions::from_mode(0o444),
                )
                .map_err(|e| e.to_string())?;
                std::fs::set_permissions(&cloned_dir, std::fs::Permissions::from_mode(0o555))
                    .map_err(|e| e.to_string())?;
                symlink(&source_dir, to.join("outside-link")).map_err(|e| e.to_string())?;
            }
            Ok(())
        });

        let cloned_dir = worktree.join("target/macos-system-profiler-exact20/run");
        let cloned_file = cloned_dir.join("evidence.txt");
        let writable = [cloned_dir.as_path(), cloned_file.as_path()]
            .into_iter()
            .all(|path| {
                std::fs::symlink_metadata(path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o200
                    != 0
            });
        let source_sealed = std::fs::symlink_metadata(&source_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777
            == 0o555
            && std::fs::symlink_metadata(&source_file)
                .unwrap()
                .permissions()
                .mode()
                & 0o777
                == 0o444;
        let bytes_match =
            std::fs::read(&source_file).unwrap() == std::fs::read(&cloned_file).unwrap();
        let removed = git_cmd(&repo)
            .args(["worktree", "remove", "--force", worktree.to_str().unwrap()])
            .run();
        if cloned_dir.exists() {
            std::fs::set_permissions(&cloned_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::set_permissions(&source_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&source_file, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(report.warnings.is_empty(), "{report:?}");
        assert!(writable, "cloned evidence must be owner-writable");
        assert!(source_sealed, "source evidence must stay sealed");
        assert!(bytes_match, "warming must preserve evidence bytes");
        assert!(removed.is_ok(), "{removed:?}");
    }

    #[test]
    fn warming_leaves_a_tracked_directory_to_git() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join("tracked.txt"), "parent edit\n").unwrap();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert_eq!(
            std::fs::read_to_string(worktree.join("tracked.txt")).unwrap(),
            "tracked\n"
        );
    }

    #[test]
    fn warming_never_copies_an_ignored_file() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(!worktree.join(".env").exists());
    }

    #[test]
    fn warming_preserves_an_existing_target() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::create_dir_all(worktree.join("build/cache")).unwrap();
        std::fs::write(worktree.join("build/cache/data"), "workspace").unwrap();

        let report = warm_worktree_with(&repo, &worktree, |_, _| {
            panic!("existing target must not be copied over")
        });

        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "workspace"
        );
    }

    #[test]
    fn a_slow_warm_finishes_without_changing_its_report() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, |from, to| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            plain_copy(from, to)
        });

        assert_eq!(report.warmed, 1);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "warm"
        );
    }

    #[test]
    fn warming_copies_sidecars_declared_by_tauri_config() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join(".gitignore"), "build/\n").unwrap();
        std::fs::create_dir_all(repo.join("src-tauri/binaries")).unwrap();
        std::fs::write(
            repo.join("src-tauri/tauri.conf.json"),
            r#"{"bundle":{"externalBin":["binaries/bridge"]}}"#,
        )
        .unwrap();
        std::fs::write(
            repo.join("src-tauri/binaries/bridge-aarch64-apple-darwin"),
            "sidecar",
        )
        .unwrap();

        let report = warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(report.warnings.is_empty(), "{report:?}");
        assert_eq!(
            std::fs::read_to_string(
                worktree.join("src-tauri/binaries/bridge-aarch64-apple-darwin")
            )
            .unwrap(),
            "sidecar"
        );
    }

    #[test]
    fn warming_excludes_tmp_directories() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join(".gitignore"), "build/\n.tmp/\n").unwrap();
        std::fs::create_dir_all(repo.join(".tmp/cache")).unwrap();
        std::fs::write(repo.join(".tmp/cache/state"), "scratch").unwrap();

        let report = warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(report.warnings.is_empty(), "{report:?}");
        assert!(!worktree.join(".tmp").exists());
    }

    #[test]
    fn warming_excludes_mdkb_directories() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join(".gitignore"), "build/\n.mdkb/\n").unwrap();
        std::fs::create_dir_all(repo.join(".mdkb/store")).unwrap();
        std::fs::write(repo.join(".mdkb/store/data"), "index").unwrap();

        let report = warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(report.warnings.is_empty(), "{report:?}");
        assert!(!worktree.join(".mdkb").exists());
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "warm"
        );
    }

    #[test]
    fn warming_leaves_the_worktree_git_link_intact() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(worktree.join(".git").is_file());
        git_cmd(&worktree)
            .args(["status", "--porcelain"])
            .run()
            .unwrap();
    }

    #[test]
    fn a_failed_copy_reports_a_cold_directory_instead_of_failing() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, |_, _| Err("refused".into()));
        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("build"));
        assert!(report.warnings[0].contains("cold"));
        assert!(worktree.join(".git").is_file());
    }

    #[test]
    fn a_parent_with_no_ignored_directories_warms_nothing_and_says_nothing() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, _| {
            panic!(
                "nothing should have been copied, but '{}' was",
                from.display()
            )
        });

        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
    }

    #[test]
    fn warming_skips_the_ignored_directory_that_holds_the_destination() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::write(repo.join(".gitignore"), "/.worktrees\n").unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "ignore",
            ])
            .run()
            .unwrap();
        let worktree = repo.join(".worktrees/feature");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, _| {
            panic!(
                "'{}' contains the destination and must never be copied",
                from.display()
            )
        });

        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
    }

    #[test]
    fn warming_skips_an_ignored_directory_that_is_its_own_repository() {
        let (_temp, repo, worktree) = warming_fixture();
        git_cmd(&repo.join("build")).args(["init"]).run().unwrap();
        let report = warm_worktree_with(&repo, &worktree, |_, _| panic!("must not copy"));
        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings, [] as [std::string::String; 0]);
    }

    /// Story 1398-42b4 criterion 2: a child created from a linked-worktree
    /// source receives the source's ignored build inputs, not only a passing
    /// probe. Catches a fix that unblocks the probe while candidate discovery
    /// still reads the source through `.git/` as a directory.
    #[test]
    fn a_linked_worktree_source_warms_a_sibling_worktree() {
        let (temp, _repo, worktree) = warming_fixture();
        std::fs::write(worktree.join(".gitignore"), "build/\n").unwrap();
        std::fs::create_dir_all(worktree.join("build/cache")).unwrap();
        std::fs::write(worktree.join("build/cache/data"), "from-worktree").unwrap();
        let child = temp.path().join("child");
        git_cmd(&worktree)
            .args([
                "worktree",
                "add",
                "-b",
                "grandchild",
                child.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&worktree, &child, plain_copy);

        assert_eq!(report.warmed, 1, "{report:?}");
        assert_eq!(
            std::fs::read_to_string(child.join("build/cache/data")).unwrap(),
            "from-worktree"
        );
    }

    /// A relative `gitdir:` pointer (git `worktree.useRelativePaths`, and
    /// submodules) resolves against the source directory. Catches resolving it
    /// against the process working directory.
    #[test]
    fn head_file_resolves_a_relative_gitdir_pointer() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(temp.path().join("store/wt")).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(temp.path().join("store/wt/HEAD"), "ref: refs/heads/x\n").unwrap();
        std::fs::write(src.join(".git"), "gitdir: ../store/wt\n").unwrap();

        assert!(head_file(&src).is_file(), "{:?}", head_file(&src));
    }

    #[test]
    fn a_probe_without_a_head_says_so_instead_of_probing() {
        let temp = TempDir::new().unwrap();
        let bare = temp.path().join("not-a-repo");
        std::fs::create_dir_all(&bare).unwrap();

        let CowSupport::Unsupported(reason) = probe_cow_support(&bare, temp.path()) else {
            panic!("a directory without .git/HEAD cannot be probed");
        };

        assert!(reason.contains("no HEAD to probe"), "{reason}");
    }

    #[test]
    fn a_probe_on_one_volume_never_reports_a_volume_mismatch() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);

        if let CowSupport::Unsupported(reason) = probe_cow_support(&repo, temp.path()) {
            assert!(!reason.contains("different volumes"), "{reason}");
        }
    }

    #[test]
    fn a_copy_succeeds_only_when_cp_succeeds_and_the_target_exists() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        std::fs::write(&source, "payload").unwrap();

        assert!(clone_file_with(&[], &source, &target));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "payload");

        // cp fails on a missing source; a target left from earlier must not
        // make the attempt look successful.
        assert!(!clone_file_with(&[], &temp.path().join("missing"), &target));
    }

    #[test]
    fn the_nearest_existing_ancestor_is_found_above_missing_directories() {
        let temp = TempDir::new().unwrap();
        let existing = temp.path().join("exists");
        std::fs::create_dir_all(&existing).unwrap();

        assert_eq!(existing_ancestor(&existing), Some(existing.clone()));
        assert_eq!(
            existing_ancestor(&existing.join("missing/deeper")),
            Some(existing)
        );
    }

    #[cfg(unix)]
    #[test]
    fn same_volume_compares_devices_and_is_unknown_for_a_missing_path() {
        let temp = TempDir::new().unwrap();
        let inner = temp.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();

        assert_eq!(same_volume(temp.path(), &inner), Some(true));
        assert_eq!(same_volume(temp.path(), &temp.path().join("missing")), None);
        assert_eq!(same_volume(&temp.path().join("missing"), temp.path()), None);
    }

    #[test]
    fn a_failed_tree_clone_reports_the_error() {
        let temp = TempDir::new().unwrap();

        let error = clone_tree(&temp.path().join("missing"), &temp.path().join("dest"))
            .expect_err("a missing source cannot be cloned");

        assert!(error.contains("copy-on-write copy of"), "{error}");
    }

    #[test]
    fn production_warming_reports_a_copy_or_a_reason_for_an_ignored_directory() {
        let (_temp, repo, worktree) = warming_fixture();

        let report = warm_worktree(&repo, &worktree);

        assert_ne!(report, WarmingReport::default());
        assert!(
            report.warmed == 1 || report.warnings.len() == 1,
            "{report:?}"
        );
    }

    #[test]
    fn only_files_named_after_the_declared_sidecar_are_candidates() {
        let temp = TempDir::new().unwrap();
        let src = temp.path();
        std::fs::create_dir_all(src.join("src-tauri/binaries/bridge-dir")).unwrap();
        std::fs::write(
            src.join("src-tauri/tauri.conf.json"),
            r#"{"bundle":{"externalBin":["binaries/bridge"]}}"#,
        )
        .unwrap();
        std::fs::write(src.join("src-tauri/binaries/bridge-arm"), "x").unwrap();
        std::fs::write(src.join("src-tauri/binaries/unrelated"), "x").unwrap();

        assert_eq!(
            external_bin_candidates(src),
            vec![PathBuf::from("src-tauri/binaries/bridge-arm")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn warming_never_copies_a_symlinked_sidecar() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join(".gitignore"), "").unwrap();
        std::fs::remove_dir_all(repo.join("build")).unwrap();
        std::fs::create_dir_all(repo.join("src-tauri/binaries")).unwrap();
        std::fs::write(
            repo.join("src-tauri/tauri.conf.json"),
            r#"{"bundle":{"externalBin":["binaries/bridge"]}}"#,
        )
        .unwrap();
        std::fs::write(repo.join("real-sidecar"), "x").unwrap();
        std::os::unix::fs::symlink(
            repo.join("real-sidecar"),
            repo.join("src-tauri/binaries/bridge-link"),
        )
        .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, _| {
            panic!("a symlink must not be copied: {}", from.display())
        });

        assert_eq!(report.warmed, 0);
    }

    #[test]
    fn warm_artifacts_reports_each_present_directory_with_its_size() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("node_modules/pkg")).unwrap();
        std::fs::write(
            temp.path().join("node_modules/pkg/index.js"),
            vec![0u8; 2048],
        )
        .unwrap();

        assert_eq!(
            warm_artifacts(temp.path()),
            vec![WarmArtifact {
                path: "node_modules".into(),
                size: "2.0K".into()
            }]
        );
    }

    #[test]
    fn walk_size_sums_file_lengths_and_measures_a_plain_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("one");
        std::fs::write(&file, vec![0u8; 1500]).unwrap();
        std::fs::create_dir_all(temp.path().join("sub")).unwrap();
        std::fs::write(temp.path().join("sub/two"), vec![0u8; 500]).unwrap();

        assert_eq!(walk_size(&file), Some(1500));
        assert_eq!(walk_size(temp.path()), Some(2000));
        assert_eq!(walk_size(&temp.path().join("missing")), None);
    }

    #[test]
    fn human_size_stops_at_the_largest_unit() {
        assert_eq!(human_size(2048 * 1024 * 1024 * 1024 * 1024), "2048T");
    }
}
