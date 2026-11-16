//! Temporary locations shared by repository repair and orphan cleanup.

use std::path::{Component, Path, PathBuf};

/// OS-configured temporary directories and the scratch roots used by TUIC tooling.
pub fn recognized_temp_roots() -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    for key in ["TMPDIR", "TMP", "TEMP"] {
        if let Some(root) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(root));
        }
    }
    if cfg!(unix) {
        roots.extend(
            [
                "/tmp",
                "/private/tmp",
                "/var/folders",
                "/private/var/folders",
            ]
            .map(PathBuf::from),
        );
    }
    roots
}

fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Resolve existing ancestors too: a missing checkout below a symlink into a
/// temporary root must not become eligible for registration cleanup.
fn resolved(path: &Path) -> PathBuf {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return normalized(&canonical);
        }
        let Some(name) = ancestor.file_name() else {
            return normalized(path);
        };
        suffix.push(name);
        let Some(parent) = ancestor.parent() else {
            return normalized(path);
        };
        ancestor = parent;
    }
}

/// Match components, not string prefixes, in both registered and resolved forms.
/// A symlink out of a scratch directory is still a temporary registration.
pub fn is_under_temp_roots(path: &Path, roots: &[PathBuf]) -> bool {
    let lexical = normalized(path);
    let canonical = resolved(path);
    roots.iter().filter(|root| root.is_absolute()).any(|root| {
        let root_lexical = normalized(root);
        let root_canonical = resolved(root);
        lexical.starts_with(&root_lexical) || canonical.starts_with(&root_canonical)
    })
}

/// Whether `path` is a *persistent* test fixture that happens to sit under a temp root.
///
/// Tests of orphan cleanup and registration repair need a checkout the classifier
/// treats as persistent, but the only writable scratch an agent sandbox offers is under
/// `$TMPDIR`, which is a recognised temp root. The fixture root
/// (`tuic_test_support::non_temporary_fixture_root`, below the per-run test root) is
/// therefore exempted here. Compiled in only with the `test-support` feature, which
/// the app and `tuic-git` enable for their test builds; in every other build this is a
/// constant `false`, so production classification is exactly [`recognized_temp_roots`].
#[cfg(feature = "test-support")]
pub fn is_persistent_test_fixture(path: &Path) -> bool {
    is_under_temp_roots(path, &[tuic_test_support::non_temporary_fixture_root()])
}

#[cfg(not(feature = "test-support"))]
pub fn is_persistent_test_fixture(_path: &Path) -> bool {
    false
}

pub fn is_temporary_path(path: &Path) -> bool {
    is_under_temp_roots(path, &recognized_temp_roots()) && !is_persistent_test_fixture(path)
}
