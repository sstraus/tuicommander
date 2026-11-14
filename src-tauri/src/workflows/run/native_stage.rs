//! The controlled implementer's only write path: listed workspace files, staged.
//! No shell, process or broad filesystem privilege; review and commit stay in the
//! workflow body (Boss Q7=A, 2026-11-12).
use super::check::{git_output, staged_artifact};
use super::native_operations::NativeContext;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

pub(super) const HANDLER: &str = "native.workspace.stage";
const MAX_FILES: usize = 64;
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StageFile {
    pub path: String,
    /// New UTF-8 content; `None` deletes the tracked file.
    pub content: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StageRequest {
    pub files: Vec<StageFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct StageReceipt {
    pub base: String,
    pub tree: String,
    pub paths: Vec<String>,
}

impl StageRequest {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            return Err(format!("stage requires 1 to {MAX_FILES} files"));
        }
        let bytes: usize = self
            .files
            .iter()
            .map(|f| f.path.len() + f.content.as_ref().map_or(0, String::len))
            .sum();
        if bytes > MAX_BYTES {
            return Err("stage request exceeds 4 MiB".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for file in &self.files {
            relative(&file.path)?;
            if !seen.insert(file.path.as_str()) {
                return Err("stage lists a path twice".into());
            }
        }
        Ok(())
    }
}

/// Only plain relative components; never absolute, `..`, `.git` or NUL.
fn relative(path: &str) -> Result<&Path, String> {
    let value = Path::new(path);
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\0')
        || path.contains('\\')
        || crate::fs::is_absolute_on_any_platform(path)
        || !value.components().all(|c| match c {
            Component::Normal(part) => part != ".git",
            _ => false,
        })
    {
        return Err(format!("stage path is not a plain workspace path: {path}"));
    }
    Ok(value)
}

/// Refuse a path whose existing ancestors or itself is a symlink.
fn plain(root: &Path, path: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("stage path passes through a symlink".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub(super) fn stage(
    context: &NativeContext,
    request: &StageRequest,
) -> Result<StageReceipt, String> {
    context.current(None)?;
    context.git_workspace()?;
    request.validate()?;
    let root = context.workspace_path();
    // Foreign dirt is never staged or attributed to this call.
    staged_artifact(root)
        .map_err(|_| "stage requires a workspace without unstaged or untracked changes")?;
    for file in &request.files {
        let path = relative(&file.path)?;
        plain(root, path)?;
        let target = root.join(path);
        match &file.content {
            Some(content) => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&target, content).map_err(|e| e.to_string())?;
                git_output(root, &["add", "--", &file.path])?;
            }
            None => {
                git_output(root, &["rm", "-q", "--", &file.path])
                    .map_err(|_| format!("stage cannot delete untracked path: {}", file.path))?;
            }
        }
    }
    let (base, tree) = staged_artifact(root)?;
    Ok(StageReceipt {
        base,
        tree,
        paths: request.files.iter().map(|f| f.path.clone()).collect(),
    })
}
