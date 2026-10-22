//! Private, unpredictable scratch directories for sensitive files.
//!
//! A fixed name under the temp dir (`temp_dir()/mdkb-install`,
//! `temp_dir()/tuicommander-certs`) is a shared, guessable path wherever
//! `TMPDIR` is unset — `/tmp` on most Linux systems. Another local user can
//! pre-create it, keep it writable, and swap or read what we put there: a
//! binary that is about to be copied with root privileges, or a TLS private
//! key. Everything here instead lives in a fresh `mkdtemp`-style directory
//! (random name, mode 0700 on Unix) that is removed when its guard drops,
//! including on every error path.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A fresh private directory under `base` whose name starts with `prefix`.
/// Never reuses an existing directory: the random suffix is created with
/// `mkdir` semantics, so a pre-created directory of the same prefix is
/// skipped, never adopted.
pub(crate) fn private_tempdir_in(base: &Path, prefix: &str) -> io::Result<tempfile::TempDir> {
    let dir = tempfile::Builder::new().prefix(prefix).tempdir_in(base)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // tempfile already creates 0700; pin it so a permissive umask or a
        // future default can never widen it.
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// Write `bytes` to a new file `name` inside `dir` (which must be private),
/// refusing to open anything that already exists (no symlink or planted file
/// is ever followed), then read it back and compare, so the caller knows the
/// staged content is exactly what it downloaded.
pub(crate) fn write_private_file(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    #[cfg_attr(not(unix), allow(unused_variables))] mode: u32,
) -> io::Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a plain file name: {name:?}"),
        ));
    }
    let path = dir.join(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    let mut file = options.open(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    let meta = std::fs::symlink_metadata(&path)?;
    if !meta.file_type().is_file() {
        return Err(io::Error::other(format!(
            "staged {} is no longer a regular file",
            path.display()
        )));
    }
    if std::fs::read(&path)? != bytes {
        return Err(io::Error::other(format!(
            "staged {} does not match what was written",
            path.display()
        )));
    }
    Ok(path)
}

/// A file staged in its own private directory; dropping it removes both.
pub(crate) struct StagedFile {
    path: PathBuf,
    // Field order: the path is only meaningful while the dir lives.
    _dir: tempfile::TempDir,
}

impl StagedFile {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Stage a downloaded binary for an elevated copy (`copy_with_elevation`):
/// a fresh private dir under `base` (production: `std::env::temp_dir()`)
/// holding `file_name` with mode 0700.
pub(crate) fn stage_for_elevated_copy(
    base: &Path,
    prefix: &str,
    file_name: &str,
    bytes: &[u8],
) -> io::Result<StagedFile> {
    let dir = private_tempdir_in(base, prefix)?;
    let path = write_private_file(dir.path(), file_name, bytes, 0o700)?;
    Ok(StagedFile { path, _dir: dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn staging_uses_a_fresh_private_dir_and_never_the_predictable_one() {
        let base = tempfile::tempdir().unwrap();
        // An attacker's pre-created, world-writable predictable dir with a
        // planted binary in it.
        let predictable = base.path().join("mdkb-install");
        std::fs::create_dir(&predictable).unwrap();
        std::fs::write(predictable.join("mdkb-linux-x64"), b"evil").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&predictable, std::fs::Permissions::from_mode(0o777)).unwrap();
        }

        let first =
            stage_for_elevated_copy(base.path(), "mdkb-install-", "mdkb-linux-x64", b"real")
                .unwrap();
        let second =
            stage_for_elevated_copy(base.path(), "mdkb-install-", "mdkb-linux-x64", b"real")
                .unwrap();
        let first_dir = first.path().parent().unwrap().to_path_buf();
        let second_dir = second.path().parent().unwrap().to_path_buf();

        assert_ne!(first_dir, predictable);
        assert_ne!(second_dir, predictable);
        assert_ne!(first_dir, second_dir, "every staging dir is unique");
        assert_eq!(first_dir.parent().unwrap(), base.path());
        assert_eq!(std::fs::read(first.path()).unwrap(), b"real");
        // The planted file is untouched and was never used.
        assert_eq!(
            std::fs::read(predictable.join("mdkb-linux-x64")).unwrap(),
            b"evil"
        );
        #[cfg(unix)]
        {
            assert_eq!(mode_of(&first_dir), 0o700);
            assert_eq!(mode_of(first.path()), 0o700);
        }

        drop(first);
        assert!(!first_dir.exists(), "dropping the stage removes its dir");
        assert!(second_dir.exists());
        drop(second);
        assert!(!second_dir.exists());
        assert!(predictable.exists(), "a dir we do not own is never removed");
    }

    #[test]
    fn write_private_file_refuses_existing_files_and_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("taken"), b"x").unwrap();
        let err = write_private_file(dir.path(), "taken", b"y", 0o600).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(dir.path().join("taken")).unwrap(), b"x");
        for bad in ["", ".", "..", "a/b", "a\\b"] {
            assert!(
                write_private_file(dir.path(), bad, b"y", 0o600).is_err(),
                "{bad:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn write_private_file_never_follows_a_planted_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"victim").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("link")).unwrap();
        assert!(write_private_file(dir.path(), "link", b"y", 0o600).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"victim");
    }

    #[test]
    fn staged_dir_is_removed_on_panic() {
        let base = tempfile::tempdir().unwrap();
        let seen = std::sync::Mutex::new(None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let staged =
                stage_for_elevated_copy(base.path(), "mdkb-install-", "bin", b"x").unwrap();
            *seen.lock().unwrap() = Some(staged.path().parent().unwrap().to_path_buf());
            panic!("elevated copy blew up");
        }));
        assert!(result.is_err());
        let dir = seen.lock().unwrap().clone().unwrap();
        assert!(
            !dir.exists(),
            "unwinding drops the guard and removes the dir"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_tempdir_is_0700_even_under_a_permissive_umask() {
        // The mode is pinned explicitly, so it does not depend on the process umask.
        let base = tempfile::tempdir().unwrap();
        let dir = private_tempdir_in(base.path(), "p-").unwrap();
        assert_eq!(mode_of(dir.path()), 0o700);
    }

    #[test]
    fn private_tempdir_fails_when_base_is_missing() {
        let base = tempfile::tempdir().unwrap();
        assert!(private_tempdir_in(&base.path().join("absent"), "p-").is_err());
    }
}
