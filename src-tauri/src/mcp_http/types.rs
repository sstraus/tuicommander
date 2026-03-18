use serde::{Deserialize, Serialize};

// --- Request/Response types ---

#[derive(Serialize)]
pub(super) struct HealthResponse {
    pub ok: bool,
}

#[derive(Serialize)]
pub(super) struct SessionInfo {
    pub session_id: String,
    pub cwd: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    // Session state (from accumulator) — present when broadcast channel is active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::state::SessionState>,
}

#[derive(Deserialize)]
pub(super) struct CreateSessionRequest {
    pub rows: Option<u16>,
    pub cols: Option<u16>,
    pub shell: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct WriteRequest {
    pub data: String,
}

#[derive(Deserialize)]
pub(super) struct SetNameRequest {
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ResizeRequest {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Deserialize)]
pub(super) struct OutputQuery {
    pub limit: Option<usize>,
    /// When set to "text", ANSI escape sequences are stripped from the output.
    pub format: Option<String>,
    /// Starting offset for log-mode WebSocket catch-up (skip lines already fetched via HTTP).
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct PathQuery {
    pub path: String,
}

#[derive(Deserialize, Default)]
pub(super) struct OptionalRepoQuery {
    #[serde(default, rename = "repoPath")]
    pub repo_path: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct CiChecksQuery {
    pub path: String,
    pub pr_number: i64,
}

#[derive(Deserialize)]
pub(super) struct PrDiffQuery {
    pub path: String,
    pub pr: i64,
}

#[derive(Deserialize)]
pub(super) struct ApprovePrRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
}

#[derive(Deserialize)]
pub(super) struct SpawnAgentRequest {
    pub rows: Option<u16>,
    pub cols: Option<u16>,
    pub cwd: Option<String>,
    pub prompt: String,
    pub model: Option<String>,
    pub print_mode: Option<bool>,
    pub output_format: Option<String>,
    pub agent_type: Option<String>,
    pub binary_path: Option<String>,
    pub args: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(super) struct HashPasswordRequest {
    pub password: String,
}

#[derive(Deserialize)]
pub(super) struct CreateWorktreeRequest {
    pub base_repo: String,
    pub branch_name: String,
    /// Optional start point (commit/branch). Defaults to HEAD when omitted.
    pub base_ref: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RemoveWorktreeQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    /// When true, also delete the local branch. Defaults to true.
    #[serde(rename = "deleteBranch", default)]
    pub delete_branch: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct GenerateWorktreeNameRequest {
    pub existing_names: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct FileQuery {
    pub path: String,
    pub file: String,
    pub scope: Option<String>,
    pub untracked: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct RenameBranchRequest {
    pub path: String,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Deserialize)]
pub(super) struct NameQuery {
    pub name: String,
}

#[derive(Deserialize)]
pub(super) struct BranchQuery {
    pub branch: String,
}

#[derive(Deserialize)]
pub(super) struct ProcessPromptRequest {
    pub content: String,
    pub variables: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub(super) struct ExtractVariablesRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub(super) struct DetectBinaryQuery {
    pub binary: String,
}

#[derive(Deserialize)]
pub(super) struct CreateSessionWithWorktreeRequest {
    pub config: CreateSessionRequest,
    pub base_repo: String,
    pub branch_name: String,
}

// --- File browser types ---

#[derive(Deserialize)]
pub(super) struct FsDirQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub subdir: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct FsFileQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub file: String,
}

#[derive(Deserialize)]
pub(super) struct FsSearchQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct FsSearchContentQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub query: String,
    #[serde(rename = "caseSensitive")]
    pub case_sensitive: Option<bool>,
    #[serde(rename = "useRegex")]
    pub use_regex: Option<bool>,
    #[serde(rename = "wholeWord")]
    pub whole_word: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct FsExternalFileQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub(super) struct FsWriteFileRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub file: String,
    pub content: String,
}

#[derive(Deserialize)]
pub(super) struct FsDirCreateRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub dir: String,
}

#[derive(Deserialize)]
pub(super) struct FsPathRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub path: String,
}

#[derive(Deserialize)]
pub(super) struct FsRenameRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub(super) struct FsCopyRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub(super) struct FsGitignoreRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub pattern: String,
}

#[derive(Deserialize)]
pub(super) struct FinalizeMergeRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    /// "archive" or "delete"
    pub action: String,
}

#[derive(Deserialize)]
pub(super) struct CheckoutRemoteRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
}

#[derive(Deserialize)]
pub(super) struct RemoveOrphanRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "worktreePath")]
    pub worktree_path: String,
}

#[derive(Deserialize)]
pub(super) struct MergePrRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
    /// "merge", "squash", or "rebase"
    #[serde(rename = "mergeMethod")]
    pub merge_method: String,
}

// --- Recent commits query ---

#[derive(Deserialize)]
pub(super) struct RecentCommitsQuery {
    pub path: String,
    pub count: Option<u32>,
}

// --- Batch PR statuses ---

#[derive(Deserialize)]
pub(super) struct GetAllPrStatusesRequest {
    pub paths: Vec<String>,
    #[serde(default)]
    pub include_merged: bool,
}

// --- GitPanel commands ---

#[derive(Deserialize)]
pub(super) struct CommitLogQuery {
    pub path: String,
    pub count: Option<u32>,
    pub after: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct StashRefRequest {
    pub path: String,
    pub stash_ref: String,
}

#[derive(Deserialize)]
pub(super) struct FilePathQuery {
    pub path: String,
    pub file: String,
    pub count: Option<u32>,
    pub after: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct FileBlameQuery {
    pub path: String,
    pub file: String,
}

#[derive(Deserialize)]
pub(super) struct StageFilesRequest {
    pub path: String,
    pub files: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct CommitRequest {
    pub path: String,
    pub message: String,
    pub amend: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct RunGitCommandRequest {
    pub path: String,
    pub args: Vec<String>,
}
