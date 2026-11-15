//! Host-observed coarse workspace evidence. This grants no process or tool authority.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "artifact evidence awaits native installation in 008-22c4"
    )
)]
use super::check::{clean_artifact, git_output};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceMutation {
    Observe,
    Mutate,
}

/// Captured by the host before launch, never accepted from an agent report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoarseWorkspace {
    pub path: PathBuf,
    pub git_dir: String,
    pub baseline_commit: String,
    pub baseline_tree: String,
    pub mutation: WorkspaceMutation,
}

/// Content identifiers come from the retained commit, not mutable file metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoarseArtifact {
    pub commit: String,
    pub tree: String,
    pub blobs: BTreeMap<String, String>,
}

impl CoarseWorkspace {
    pub fn capture(path: &Path, mutation: WorkspaceMutation) -> Result<Self, String> {
        let scope = Self::capture_identity(path, mutation)?;
        if clean_artifact(&scope.path)?
            != (scope.baseline_commit.clone(), scope.baseline_tree.clone())
        {
            return Err("coarse workspace moved before launch; capture it again".into());
        }
        Ok(scope)
    }

    /// Native commit/review may start with a staged candidate. Capture only the
    /// checkout identity and HEAD baseline; each service validates its artifact.
    pub(super) fn capture_identity(
        path: &Path,
        mutation: WorkspaceMutation,
    ) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("coarse workspace: {e}"))?;
        let top = git_output(&path, &["rev-parse", "--show-toplevel"])?;
        if Path::new(&top).canonicalize().map_err(|e| e.to_string())? != path {
            return Err("coarse workspace must be the exact checkout root".into());
        }
        let git_dir = git_output(&path, &["rev-parse", "--absolute-git-dir"])?;
        let baseline_commit = git_output(&path, &["rev-parse", "HEAD"])?;
        let baseline_tree = git_output(&path, &["rev-parse", "HEAD^{tree}"])?;
        let scope = Self {
            path,
            git_dir,
            baseline_commit,
            baseline_tree,
            mutation,
        };
        scope.verify_identity()?;
        Ok(scope)
    }

    pub(super) fn verify_identity(&self) -> Result<(), String> {
        if self.path.canonicalize().map_err(|e| e.to_string())? != self.path
            || git_output(&self.path, &["rev-parse", "--absolute-git-dir"])? != self.git_dir
        {
            return Err(
                "coarse workspace identity changed; reconcile the original workspace".into(),
            );
        }
        Ok(())
    }

    /// Pin evidence only. The caller must separately establish the original
    /// owned process's stop and reconcile uncertain execution before completion.
    pub fn pin_artifact(&self, paths: &[String]) -> Result<CoarseArtifact, String> {
        self.verify_identity()?;
        if paths.len() > 32 {
            return Err("coarse evidence exceeds 32 artifact paths".into());
        }
        let (commit, tree) = clean_artifact(&self.path)?;
        if self.mutation == WorkspaceMutation::Observe
            && (commit != self.baseline_commit || tree != self.baseline_tree)
        {
            return Err("observational coarse activity changed its workspace".into());
        }
        git_output(
            &self.path,
            &[
                "merge-base",
                "--is-ancestor",
                &self.baseline_commit,
                &commit,
            ],
        )?;
        let mut blobs = BTreeMap::new();
        for path in paths {
            if path.is_empty()
                || path.len() > 2_000
                || crate::fs::is_absolute_on_any_platform(path)
                || path.chars().any(|c| c.is_control() || c == '\\')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
            {
                return Err("coarse artifact requires a relative file path without escapes".into());
            }
            let entry = git_output(
                &self.path,
                &["--literal-pathspecs", "ls-tree", "-z", &commit, "--", path],
            )?;
            let entry = entry
                .strip_suffix('\0')
                .ok_or("coarse artifact is absent from its commit")?;
            let (header, found) = entry
                .split_once('\t')
                .ok_or("coarse artifact is absent from its commit")?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if found != path
                || fields.len() != 3
                || fields[1] != "blob"
                || !matches!(fields[0], "100644" | "100755")
            {
                return Err("coarse artifact must be a regular committed file".into());
            }
            if blobs.insert(path.clone(), fields[2].to_owned()).is_some() {
                return Err("coarse artifact path is duplicated".into());
            }
        }
        self.verify_identity()?;
        if clean_artifact(&self.path)? != (commit.clone(), tree.clone()) {
            return Err("coarse workspace moved while pinning evidence; reconcile it".into());
        }
        Ok(CoarseArtifact {
            commit,
            tree,
            blobs,
        })
    }
}
