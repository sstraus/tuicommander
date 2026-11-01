//! How a host path is spelled: absolute on any platform, and the portable
//! `/`-separated form that git and the rest of TUIC read back.

/// Is this string an absolute path on *any* platform TUIC runs on?
///
/// `Path::is_absolute` answers for the host only, and the validators that call
/// this decide whether a path may escape a boundary. Judging a foreign shape
/// as relative is the dangerous half of that: on Windows `Path::join` replaces
/// the root when the joined path has one, so a repo path joined with
/// `/etc/passwd` lands at the root of the repo's drive rather than inside the
/// repo. So each shape the other platform uses gets an explicit string check —
/// `C:\…` and `\\…` do not parse as absolute on unix, and a leading `/` does
/// not parse as absolute on Windows, where a path without a drive letter is
/// merely rooted.
pub fn is_absolute_on_any_platform(path: &str) -> bool {
    let drive_rooted = path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && matches!(path.get(1..3), Some(":/") | Some(":\\"));
    std::path::Path::new(path).is_absolute()
        || drive_rooted
        || path.starts_with("\\\\")
        || path.starts_with('/')
}

/// Rewrite a host path string the way everything outside the Windows API
/// spells one: no `\\?\` prefix, `/` separators.
///
/// Two consumers need it and they need the same answer. `git` is one — the gix
/// and CLI adapters behind `GitReads` have to return identical bytes, and on
/// Windows `fs::canonicalize` does not oblige: it returns a verbatim `\\?\`
/// path with `\`, while `git worktree list` prints `C:/Users/…`, so a consumer
/// comparing the two finds no match at all. A git *config value* is the other:
/// there `\` starts an escape sequence, so a Windows path written verbatim
/// reaches git mangled. `/` is also the separator the rest of TUIC carries in a
/// path string (see `fs::DirEntry::path` in the app crate).
///
/// Only Windows paths are rewritten: `\` is a legal character in a unix file
/// name, so the same rewrite there would corrupt paths rather than normalise
/// them. The rewrite itself is in [`windows_portable_spelling`], which is
/// compiled and tested on every platform.
pub fn portable_spelling(path: &str) -> String {
    if cfg!(windows) {
        windows_portable_spelling(path)
    } else {
        path.to_string()
    }
}

pub fn windows_portable_spelling(path: &str) -> String {
    // `\\?\UNC\host\share` is `\\host\share` written verbatim, so the prefix
    // cannot simply be cut off that one.
    let simplified = match path.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => path.strip_prefix(r"\\?\").unwrap_or(path).to_string(),
    };
    simplified.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_rooted_forward_slash_path_is_absolute_on_every_host() {
        assert!(is_absolute_on_any_platform("C:/Users/me/repo"));
        assert!(is_absolute_on_any_platform("c:/Users/me/repo"));
        assert!(is_absolute_on_any_platform(r"C:\Users\me\repo"));
        assert!(!is_absolute_on_any_platform("C:relative/repo"));
        assert!(!is_absolute_on_any_platform("1:/Users/me/repo"));
    }

    /// The spelling half of the parity the `git_reads` shootout asserts, where
    /// it can be checked on every platform rather than only on the one that
    /// breaks.
    #[test]
    fn windows_paths_are_rewritten_the_way_git_prints_them() {
        // What `fs::canonicalize` hands back on Windows.
        assert_eq!(
            windows_portable_spelling(r"\\?\C:\Users\me\repo"),
            "C:/Users/me/repo"
        );
        // Clone destinations use the same spelling, including the CI runner drive.
        assert_eq!(windows_portable_spelling(r"\\?\D:\a\repo"), "D:/a/repo");
        // A plain host path: separators only.
        assert_eq!(
            windows_portable_spelling(r"C:\Users\me\repo"),
            "C:/Users/me/repo"
        );
        // The verbatim UNC form is `\\host\share`, not `UNC\host\share`.
        assert_eq!(
            windows_portable_spelling(r"\\?\UNC\host\share\repo"),
            "//host/share/repo"
        );
        // Already in the portable spelling: unchanged.
        assert_eq!(
            windows_portable_spelling("C:/Users/me/repo"),
            "C:/Users/me/repo"
        );
    }

    /// Catches `||` -> `&&` in `is_absolute_on_any_platform`: each shape is
    /// absolute by exactly one disjunct, so a host-independent verdict fails if
    /// any `||` becomes `&&` (`\\host\share` is relative on unix, `/etc/passwd`
    /// is merely rooted on Windows).
    #[test]
    fn each_foreign_absolute_shape_is_absolute_on_every_host() {
        assert!(is_absolute_on_any_platform(r"\\host\share\repo"));
        assert!(is_absolute_on_any_platform("/etc/passwd"));
        assert!(!is_absolute_on_any_platform("repo/sub"));
        assert!(!is_absolute_on_any_platform(r"repo\sub"));
    }

    /// Catches `portable_spelling` replaced by `String::new()` or `"xyzzy"`.
    /// Only Windows hosts rewrite; elsewhere `\` is a legal file-name character
    /// and the path must come back byte-identical.
    #[test]
    fn portable_spelling_rewrites_only_on_windows() {
        let (windows_path, posix_path) = (r"C:\Users\me\repo", "/home/me/repo");
        assert_eq!(portable_spelling(posix_path), posix_path);
        let expected = if cfg!(windows) {
            "C:/Users/me/repo"
        } else {
            windows_path
        };
        assert_eq!(portable_spelling(windows_path), expected);
    }
}
