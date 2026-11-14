//! Typed Git operations reuse native services after current exact-effect checks.
use super::RunStore;
use super::check::{clean_artifact, git_output};
use super::native_operations::NativeContext;
use super::neutral_effects::EffectKey;
use crate::agent_capabilities::Capability;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(super) enum NativeFailure {
    Refused(String),
    Uncertain(String),
}
impl From<String> for NativeFailure {
    fn from(value: String) -> Self {
        Self::Refused(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CommitRequest {
    pub parent: String,
    pub tree: String,
    pub message: String,
    pub review: EffectKey,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CommitReceipt {
    pub parent: String,
    pub commit: String,
    pub tree: String,
}

/// Coordinator decision (b), 2026-11-12, recorded for Boss: the run start covers a
/// commit on the run's own workspace when this run holds a passing staged review of
/// exactly that parent and tree, on a non-canonical branch. Any other commit, and
/// every push or publication, keeps its own exact approval.
pub(super) fn run_covered(
    context: &NativeContext,
    store: &RunStore,
    run: &str,
    request: &CommitRequest,
) -> bool {
    request.review.fence.run == run
        && story_branch(context.workspace_path())
        && super::native_review::require_review(
            context,
            store,
            &request.review,
            &request.parent,
            &request.tree,
        )
        .is_ok()
}

/// An attached branch that is neither a main-branch name nor the repository default.
fn story_branch(path: &std::path::Path) -> bool {
    let Ok(branch) = git_output(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]) else {
        return false;
    };
    !branch.is_empty()
        && !tuic_git::git::is_main_branch(&branch)
        && tuic_git::worktree::get_remote_default_branch(&path.to_string_lossy())
            .is_ok_and(|default| default.rsplit('/').next() != Some(branch.as_str()))
}

pub(super) fn commit(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    request: &CommitRequest,
) -> Result<CommitReceipt, NativeFailure> {
    let prepared = store
        .neutral_execution(&key.fence.run)?
        .effect(key)?
        .preparation
        .clone();
    if prepared.approval.is_none() {
        // Run-start coverage is re-proved at execution, never trusted from preparation.
        if !run_covered(context, store, &key.fence.run, request) {
            return Err(NativeFailure::Refused(
                "native commit is not covered by the run start; it needs its own approval".into(),
            ));
        }
        context.authorize_covered(store, key, "native.commit.create", request)?;
    } else {
        context.authorize(store, key, "native.commit.create", request, None)?;
    }
    let path = context.workspace_path();
    if request.message.trim().is_empty()
        || request.message.len() > 32_768
        || request.message.contains('\0')
        || git_output(path, &["rev-parse", "HEAD"])? != request.parent
        || git_output(path, &["write-tree"])? != request.tree
        || !git_output(path, &["diff", "--name-only"])?.is_empty()
        || !git_output(path, &["ls-files", "--others", "--exclude-standard"])?.is_empty()
    {
        return Err(NativeFailure::Refused(
            "native commit subject/index is stale or invalid".into(),
        ));
    }
    super::native_review::require_review(
        context,
        store,
        &request.review,
        &request.parent,
        &request.tree,
    )?;
    // No amend, shell text, staging or implicit commit/push authority.
    let commit = tuic_git::git::git_commit_blocking(
        path.to_string_lossy().into_owned(),
        request.message.clone(),
        Some(false),
    )
    .map_err(NativeFailure::Uncertain)?;
    let observed = clean_artifact(path).map_err(NativeFailure::Uncertain)?;
    if observed != (commit.clone(), request.tree.clone())
        || git_output(path, &["rev-parse", "HEAD^"]).map_err(NativeFailure::Uncertain)?
            != request.parent
    {
        return Err(NativeFailure::Uncertain(
            "native commit result differs from approved parent/tree".into(),
        ));
    }
    Ok(CommitReceipt {
        parent: request.parent.clone(),
        commit,
        tree: request.tree.clone(),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PushRequest {
    pub commit: String,
    pub tree: String,
    pub remote: String,
    pub target: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PushReceipt {
    pub commit: String,
    pub remote: String,
    pub target: String,
}

pub(super) fn push(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    request: &PushRequest,
) -> Result<PushReceipt, NativeFailure> {
    let state = context.authorize(
        store,
        key,
        "native.publication.push",
        request,
        Some(Capability::Push),
    )?;
    context.require_artifact(&request.commit, &request.tree)?;
    let path = context.workspace_path();
    if !request.target.starts_with("refs/heads/")
        || request.target.len() > 512
        || request.remote.trim().is_empty()
        || request.remote.starts_with('-')
        || request.remote.len() > 2_000
    {
        return Err(NativeFailure::Refused(
            "native push requires an exact remote and branch ref".into(),
        ));
    }
    let remote = std::path::Path::new(&request.remote);
    if !remote.is_absolute()
        || remote.canonicalize().map_err(|e| e.to_string())? != remote
        || git_output(remote, &["rev-parse", "--is-bare-repository"])? != "true"
    {
        return Err(NativeFailure::Refused(
            "native push requires an exact local bare repository; network publication unavailable"
                .into(),
        ));
    }
    git_output(path, &["check-ref-format", &request.target])?;
    // Literal destination, one exact commit/ref, never --all, --mirror, tags or force.
    let args = vec![
        "push".into(),
        "--porcelain".into(),
        "--".into(),
        request.remote.clone(),
        format!("{}:{}", request.commit, request.target),
    ];
    let result = crate::git::run_git_command_blocking(&state, &path.to_string_lossy(), &args);
    if !result.success {
        return Err(NativeFailure::Uncertain(result.stderr));
    }
    context
        .require_artifact(&request.commit, &request.tree)
        .map_err(NativeFailure::Uncertain)?;
    Ok(PushReceipt {
        commit: request.commit.clone(),
        remote: request.remote.clone(),
        target: request.target.clone(),
    })
}
