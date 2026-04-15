//! Path sandboxing for AI agent filesystem tools.
//!
//! Every filesystem read/write routes through `FileSandbox::resolve` or
//! `resolve_for_write`, which canonicalize + `starts_with(root)` check the
//! resolved path. Symlinks pointing outside the root are rejected because
//! `canonicalize` follows them before the check.
//!
//! TOCTOU between resolve and the actual I/O is an accepted risk — the OS
//! sandbox is the real boundary (same model as Claude Code).

use std::path::{Path, PathBuf};

/// A filesystem jail rooted at a session's repo root (or CWD fallback).
// Fields/methods become live when the filesystem tools (read_file,
// write_file, etc.) land in the next story. Keep the struct API in place so
// Step 2 can wire it up without churning this module.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FileSandbox {
    root: PathBuf,
}

#[allow(dead_code)]
impl FileSandbox {
    /// Build a sandbox. `root` must exist and be a directory — it is
    /// canonicalized so later comparisons use the same resolved form.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|e| format!("sandbox root invalid: {e}"))?;
        if !root.is_dir() {
            return Err(format!(
                "sandbox root is not a directory: {}",
                root.display()
            ));
        }
        Ok(Self { root })
    }

    /// Sandbox root (already canonicalized).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a path for read-like ops. Accepts absolute or relative paths;
    /// relative paths are joined onto the sandbox root. Canonicalizes (so
    /// symlinks are resolved) and verifies the result lies within the root.
    ///
    /// For non-existent leaf paths, canonicalizes the parent and joins the
    /// file name — this lets callers probe for files without failing.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let joined = self.join_with_root(Path::new(path));

        if let Ok(canon) = joined.canonicalize() {
            return self.check_within(canon);
        }

        let parent = joined
            .parent()
            .ok_or_else(|| format!("path has no parent: {}", joined.display()))?;
        let file_name = joined
            .file_name()
            .ok_or_else(|| format!("path has no file name: {}", joined.display()))?;
        let canon_parent = parent
            .canonicalize()
            .map_err(|e| format!("parent does not exist: {e}"))?;
        let canon_parent = self.check_within(canon_parent)?;
        Ok(canon_parent.join(file_name))
    }

    /// Resolve a path for write ops. Creates any missing parent directories
    /// (inside the sandbox) before resolving, so `write_file` can create new
    /// nested paths atomically.
    pub fn resolve_for_write(&self, path: &str) -> Result<PathBuf, String> {
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let joined = self.join_with_root(Path::new(path));
        let parent = joined
            .parent()
            .ok_or_else(|| format!("path has no parent: {}", joined.display()))?;

        // Walk up to the nearest existing ancestor and verify sandbox before
        // creating anything. This blocks creating directories via a symlinked
        // ancestor that points outside the root.
        let mut ancestor = parent.to_path_buf();
        while !ancestor.exists() {
            match ancestor.parent() {
                Some(p) if !p.as_os_str().is_empty() => ancestor = p.to_path_buf(),
                _ => return Err(format!("no existing ancestor for {}", joined.display())),
            }
        }
        let canon_ancestor = ancestor
            .canonicalize()
            .map_err(|e| format!("ancestor invalid: {e}"))?;
        self.check_within(canon_ancestor)?;

        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent dirs: {e}"))?;
        }
        self.resolve(path)
    }

    /// Heuristic binary-file detection: read up to 8KB and check for UTF-8
    /// decodability. Invalid UTF-8 ⇒ treat as binary. Read errors ⇒ false
    /// (let the caller surface the real error on the actual open).
    pub fn is_binary(path: &Path) -> bool {
        use std::io::Read;
        let Ok(mut f) = std::fs::File::open(path) else {
            return false;
        };
        let mut buf = [0u8; 8192];
        let n = f.read(&mut buf).unwrap_or(0);
        if n == 0 {
            return false;
        }
        std::str::from_utf8(&buf[..n]).is_err()
    }

    fn join_with_root(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    fn check_within(&self, canon: PathBuf) -> Result<PathBuf, String> {
        if canon.starts_with(&self.root) {
            Ok(canon)
        } else {
            Err(format!(
                "path escapes sandbox: {} not within {}",
                canon.display(),
                self.root.display()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sandbox() -> (TempDir, FileSandbox) {
        let dir = TempDir::new().unwrap();
        let sb = FileSandbox::new(dir.path()).unwrap();
        (dir, sb)
    }

    #[test]
    fn new_rejects_nonexistent_root() {
        let err = FileSandbox::new("/definitely/does/not/exist/xyz").unwrap_err();
        assert!(err.contains("invalid"));
    }

    #[test]
    fn new_rejects_file_root() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("a.txt");
        fs::write(&f, "x").unwrap();
        let err = FileSandbox::new(&f).unwrap_err();
        assert!(err.contains("not a directory"));
    }

    #[test]
    fn resolve_relative_path_within_root() {
        let (dir, sb) = sandbox();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let got = sb.resolve("a.txt").unwrap();
        assert_eq!(got, dir.path().canonicalize().unwrap().join("a.txt"));
    }

    #[test]
    fn resolve_absolute_path_within_root() {
        let (dir, sb) = sandbox();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let abs = dir.path().join("a.txt");
        let got = sb.resolve(abs.to_str().unwrap()).unwrap();
        assert_eq!(got, abs.canonicalize().unwrap());
    }

    #[test]
    fn resolve_rejects_dotdot_traversal() {
        let (_dir, sb) = sandbox();
        let err = sb.resolve("../etc/passwd").unwrap_err();
        assert!(err.contains("escapes sandbox") || err.contains("parent"));
    }

    #[test]
    fn resolve_rejects_absolute_path_outside_root() {
        let (_dir, sb) = sandbox();
        let err = sb.resolve("/etc/hosts").unwrap_err();
        assert!(err.contains("escapes sandbox") || err.contains("parent"));
    }

    #[test]
    fn resolve_rejects_empty_path() {
        let (_dir, sb) = sandbox();
        assert!(sb.resolve("").is_err());
    }

    #[test]
    fn resolve_non_existent_leaf_with_existing_parent() {
        let (dir, sb) = sandbox();
        let got = sb.resolve("new_file.txt").unwrap();
        assert_eq!(got, dir.path().canonicalize().unwrap().join("new_file.txt"));
    }

    #[test]
    fn resolve_non_existent_parent_fails() {
        let (_dir, sb) = sandbox();
        let err = sb.resolve("nope/also_nope.txt").unwrap_err();
        assert!(err.contains("parent does not exist"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_symlink_escaping_root() {
        use std::os::unix::fs::symlink;
        let (dir, sb) = sandbox();
        let link = dir.path().join("escape");
        symlink("/etc", &link).unwrap();
        let err = sb.resolve("escape/hosts").unwrap_err();
        assert!(err.contains("escapes sandbox"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_allows_symlink_pointing_inside_root() {
        use std::os::unix::fs::symlink;
        let (dir, sb) = sandbox();
        let target = dir.path().join("real.txt");
        fs::write(&target, "x").unwrap();
        let link = dir.path().join("alias.txt");
        symlink(&target, &link).unwrap();
        let got = sb.resolve("alias.txt").unwrap();
        assert_eq!(got, target.canonicalize().unwrap());
    }

    #[test]
    fn resolve_for_write_creates_missing_dirs() {
        let (dir, sb) = sandbox();
        let got = sb.resolve_for_write("nested/deep/file.txt").unwrap();
        assert!(dir.path().join("nested/deep").is_dir());
        assert_eq!(
            got,
            dir.path()
                .canonicalize()
                .unwrap()
                .join("nested/deep/file.txt")
        );
    }

    #[test]
    fn resolve_for_write_rejects_traversal() {
        let (_dir, sb) = sandbox();
        assert!(sb.resolve_for_write("../outside/file.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_for_write_rejects_symlinked_ancestor() {
        use std::os::unix::fs::symlink;
        let (dir, sb) = sandbox();
        let outside = TempDir::new().unwrap();
        let link = dir.path().join("jail_break");
        symlink(outside.path(), &link).unwrap();
        let err = sb
            .resolve_for_write("jail_break/subdir/file.txt")
            .unwrap_err();
        assert!(err.contains("escapes sandbox"));
    }

    #[test]
    fn is_binary_false_for_utf8_text() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "hello\nworld\n").unwrap();
        assert!(!FileSandbox::is_binary(&p));
    }

    #[test]
    fn is_binary_true_for_non_utf8_bytes() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.bin");
        // invalid UTF-8 byte sequence
        fs::write(&p, [0xff, 0xfe, 0x00, 0x01, 0x02, 0xc3, 0x28]).unwrap();
        assert!(FileSandbox::is_binary(&p));
    }

    #[test]
    fn is_binary_false_for_empty_file() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("empty.txt");
        fs::write(&p, "").unwrap();
        assert!(!FileSandbox::is_binary(&p));
    }

    #[test]
    fn root_returns_canonicalized_path() {
        let (dir, sb) = sandbox();
        assert_eq!(sb.root(), dir.path().canonicalize().unwrap());
    }
}
