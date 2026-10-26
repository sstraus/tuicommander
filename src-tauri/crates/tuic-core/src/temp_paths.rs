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
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Gits").join(".tmp"));
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

pub fn is_temporary_path(path: &Path) -> bool {
    is_under_temp_roots(path, &recognized_temp_roots())
}
