use serde::{Deserialize, Serialize};

// --- Request/Response types ---

#[derive(Serialize)]
pub(super) struct HealthResponse {
    pub ok: bool,
    pub uptime_secs: u64,
    pub session_count: usize,
    pub protocol_version: u32,
    pub build: Option<&'static crate::remote_deploy::assets::BuildIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survive_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
    /// Which running process answered. A remote connection compares it against
    /// its own before mirroring, so a base URL that resolves back to this very
    /// process is refused instead of looping every event through itself.
    pub instance_id: &'static str,
}

#[derive(Serialize)]
pub(super) struct VersionResponse {
    pub version: &'static str,
    pub git_hash: &'static str,
}

/// One row of the session list, on every transport.
///
/// `Deserialize` because a mirrored row comes back from a remote daemon's own
/// `GET /sessions` (#791-055e), and `serde(default)` because that daemon skips
/// every field it has nothing to say about.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SessionInfo {
    pub session_id: String,
    /// Live PTY child, not the foreground agent or one of its tool subprocesses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_pid: Option<u32>,
    pub cwd: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub display_name_is_custom: bool,
    /// The name came from the agent spawn, not from an OSC/intent title the UI
    /// synced back. A reload cannot infer it from the other fields.
    pub display_name_from_spawn: bool,
    pub is_remote: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_description: Option<String>,
    /// Terminal alias (e.g. `tu-3`). The only record of it after a WebView
    /// reload: `term-alias-assigned` fires once, at spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Live agent identity bound to this PTY; differs from session_id for a
    /// locally launched tab that registered its own TUIC_SESSION.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tuic_session: Option<String>,
    /// Session (or `$TUIC_SESSION`) of the agent that spawned this one. Published
    /// once on `session-created`, so a reload or a late browser client needs it
    /// here. Never a `pending-mcp:` placeholder: no tab can match one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<String>,
    // Session state (from accumulator) — present when broadcast channel is active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::state::SessionState>,
    /// Which remote connection this session runs on; absent for a local one.
    /// The caller needs it to tell two machines' sessions apart in one list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct CreateSessionRequest {
    pub rows: Option<u16>,
    pub cols: Option<u16>,
    pub shell: Option<String>,
    pub cwd: Option<String>,
    /// Client-provided session id. Browser clients generate the id up front and
    /// register it locally BEFORE this request so the `session-created` echo
    /// (delivered over SSE, which can beat the HTTP response) is recognized as
    /// locally-created and does not spawn a duplicate "PTY:" tab. Honored only
    /// when non-empty and not already in use; otherwise the backend mints one.
    pub session_id: Option<String>,
    /// Terminal alias this tab held before the restart (e.g. `tu-3`). Same field
    /// name and meaning as `PtyConfig::alias` on the Tauri IPC side — a restored
    /// tab keeps the address other agents already know.
    pub alias: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct WriteRequest {
    pub data: String,
}

#[derive(Deserialize)]
pub(super) struct WritePartsRequest {
    pub parts: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct SetNameRequest {
    pub name: Option<String>,
    #[serde(default, rename = "isCustom")]
    pub is_custom: Option<bool>,
}

/// Compose-panel enqueue: text delivered on the session's next idle window.
#[derive(Deserialize)]
pub(super) struct EnqueueCommandRequest {
    pub text: String,
    #[serde(default, rename = "expectedDraft")]
    pub expected_draft: Option<String>,
    #[serde(default, rename = "idempotencyKey")]
    pub idempotency_key: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ResizeRequest {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Deserialize)]
pub(super) struct OutputQuery {
    pub limit: Option<usize>,
    /// Native MCP output windows, used only by format=mcp/mcp_raw.
    pub from_line: Option<usize>,
    pub from_byte: Option<u64>,
    pub byte_budget: Option<u64>,
    pub continuation: Option<String>,
    pub since_cursor: Option<usize>,
    /// When set to "text", ANSI escape sequences are stripped from the output.
    pub format: Option<String>,
    /// Starting offset for log-mode WebSocket catch-up (skip lines already fetched via HTTP).
    pub offset: Option<usize>,
    /// Content encoding the client can decode on the stream WebSocket. Only
    /// `deflate` is offered; anything else, including absent, leaves the frames
    /// in the original untagged framing. See `mcp_http::ws_compression`.
    pub compress: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct PathQuery {
    pub path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProgressViewedQuery {
    pub path: String,
    pub pty_id: Option<String>,
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
pub(super) struct ChangelogQuery {
    pub path: String,
    #[serde(default, rename = "sinceTag")]
    pub since_tag: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ConflictAssistRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
}

/// `POST /repo/pr-review` — the same pair as a conflict assist, kept separate
/// so neither route grows a field the other one has to ignore.
#[derive(Deserialize)]
pub(super) struct PrReviewRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
}

/// `POST /repo/improvement-scan`.
#[derive(Deserialize)]
pub(super) struct ImprovementScanRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub focus: crate::improvement_scan::ImprovementFocus,
}

/// `POST /repo/create-issue-from-proposal`.
///
/// The proposal travels whole rather than by id: nothing on this side stores a
/// scan, so an id would name a thing only the caller has.
#[derive(Deserialize)]
pub(super) struct CreateIssueFromProposalRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub proposal: crate::improvement_scan::ImprovementProposal,
}

#[derive(Deserialize)]
pub(super) struct UpdatePrBranchRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
    /// Head commit the user saw; GitHub refuses the update if the PR head moved.
    #[serde(rename = "expectedHeadSha")]
    pub expected_head_sha: String,
}

#[derive(Deserialize)]
pub(super) struct ApprovePrRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
}

#[derive(Deserialize)]
pub(super) struct CreatePrRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Deserialize)]
pub(super) struct CreateIssueRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub title: String,
    pub body: String,
}

#[derive(Deserialize)]
pub(super) struct PostPrReviewRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "prNumber")]
    pub pr_number: i64,
    pub body: String,
    pub event: Option<String>,
    #[serde(default)]
    pub comments: Vec<crate::github::PrReviewInlineComment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PrepareAgentLaunchArgsRequest {
    pub agent_type: String,
    pub binary_path: String,
    #[serde(default)]
    pub args: Vec<String>,
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
    /// When true, also delete the local branch. Defaults to true unless force is true.
    #[serde(rename = "deleteBranch", default)]
    pub delete_branch: Option<bool>,
    /// Permit discarding dirty workspace files. Does not override a lock or
    /// bypass branch deletion proof.
    #[serde(default)]
    pub force: Option<bool>,
    /// Explicit confirmation to remove a locked worktree.
    #[serde(rename = "overrideLock", default)]
    pub override_lock: Option<bool>,
    #[serde(rename = "expectedFingerprint", default)]
    pub expected_fingerprint: Option<String>,
    #[serde(rename = "confirmMissingCheckout", default)]
    pub confirm_missing_checkout: Option<bool>,
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
pub(super) struct FsSearchContentAllQuery {
    pub query: String,
    #[serde(rename = "caseSensitive")]
    pub case_sensitive: Option<bool>,
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
pub(super) struct FsWriteIfUnchangedRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub file: String,
    pub expected: String,
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
pub(super) struct FsResolveTerminalPathQuery {
    pub cwd: String,
    pub candidate: String,
}

#[derive(Deserialize)]
pub(super) struct FsResolveTerminalPathsRequest {
    pub cwd: String,
    pub candidates: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FsResolveMarkdownLinkRequest {
    pub root: String,
    pub current_file: String,
    pub href: String,
}

#[derive(Deserialize)]
pub(super) struct FsWarmIndexRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct FsExternalWriteRequest {
    pub path: String,
    pub content: String,
}

/// Absolute-path single-file copy/move (FileBrowser cross-repo cut/paste).
#[derive(Deserialize)]
pub(super) struct FsAbsTransferRequest {
    pub from: String,
    pub to: String,
}

/// Bulk OS drag-drop transfer (move/copy) into a destination directory.
#[derive(Deserialize)]
pub(super) struct FsTransferPathsRequest {
    #[serde(rename = "destDir")]
    pub dest_dir: String,
    pub paths: Vec<String>,
    pub mode: crate::fs::TransferMode,
    #[serde(rename = "allowRecursive")]
    pub allow_recursive: bool,
}

#[derive(Deserialize)]
pub(super) struct FinalizeMergeRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    /// Checkout to archive or delete. The merge already happened, so no branch
    /// is needed here — only which workspace to dispose of (#726-5ac7).
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
    /// "archive" or "delete"
    pub action: String,
    /// Skip the pre-flight guard that refuses to destroy a dirty worktree.
    #[serde(default)]
    pub force: Option<bool>,
    #[serde(rename = "expectedFingerprint", default)]
    pub expected_fingerprint: Option<String>,
}

/// One workspace, addressed by id, for the read-only queries.
#[derive(Deserialize)]
pub(super) struct WorkspaceIdQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
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
    #[serde(rename = "safeOnly", default)]
    pub safe_only: bool,
    /// Session ids the user saw when confirming; a live session outside this
    /// list refuses a `safeOnly` false removal.
    #[serde(rename = "confirmedSessions", default)]
    pub confirmed_sessions: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct BeginOrphanCleanupRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct AnswerOrphanCleanupRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub decision: String,
}

#[derive(Deserialize)]
pub(super) struct ClearOrphanCleanupRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    /// The dialog ended in Keep; other clients must see that instead of a cleared entry.
    #[serde(default)]
    pub kept: bool,
}

#[derive(Deserialize)]
pub(super) struct SwitchBranchRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    pub force: bool,
    pub stash: bool,
}

#[derive(Deserialize)]
pub(super) struct MergeArchiveRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
    #[serde(rename = "targetBranch")]
    pub target_branch: String,
    /// "archive", "delete", or "ask"
    #[serde(rename = "afterMerge")]
    pub after_merge: String,
    /// Skip the pre-flight guard that refuses to destroy a dirty worktree.
    #[serde(default)]
    pub force: Option<bool>,
    #[serde(rename = "expectedFingerprint", default)]
    pub expected_fingerprint: Option<String>,
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
    /// Head commit the user reviewed; GitHub refuses the merge if the PR head moved.
    #[serde(rename = "expectedHeadSha")]
    pub expected_head_sha: String,
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

// --- GitHub Issues ---

#[derive(Deserialize)]
pub(super) struct IssuesQuery {
    pub path: String,
    #[serde(default = "default_issue_filter")]
    pub filter: String,
}

fn default_issue_filter() -> String {
    "assigned".to_string()
}

#[derive(Deserialize)]
pub(super) struct IssueActionRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "issueNumber")]
    pub issue_number: i64,
}

#[derive(Deserialize)]
pub(super) struct IssueDetailQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "issueNumber")]
    pub issue_number: i64,
}

// --- GitHub auth / misc ---

#[derive(Deserialize)]
pub(super) struct CiFailureLogsQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    pub branch: String,
    #[serde(rename = "checkUrl")]
    pub check_url: Option<String>,
    #[serde(rename = "headSha")]
    pub head_sha: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct GithubSetHideDraftsRequest {
    pub hide: bool,
}

#[derive(Deserialize)]
pub(super) struct GithubPollLoginRequest {
    #[serde(rename = "deviceCode")]
    pub device_code: String,
}

#[derive(Deserialize)]
pub(super) struct GithubRemoveAccountRequest {
    pub id: String,
}

#[derive(Deserialize)]
pub(super) struct GithubBindRepoRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "remoteName")]
    pub remote_name: String,
}

#[derive(Deserialize)]
pub(super) struct GithubRepoPathBody {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct GithubResolveRepoQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct GithubResolveReposRequest {
    #[serde(rename = "repoPaths")]
    pub repo_paths: Vec<String>,
}

// --- Config / themes / notes / misc (story 066) ---

#[derive(Deserialize)]
pub(super) struct SaveRepoLocalConfigRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct SetBranchLabelRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    pub label: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct SaveNoteImageRequest {
    #[serde(rename = "noteId")]
    pub note_id: String,
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
    pub extension: String,
}

#[derive(Deserialize)]
pub(super) struct DeleteNoteAssetsRequest {
    #[serde(rename = "noteId")]
    pub note_id: String,
}

#[derive(Deserialize)]
pub(super) struct DeleteNoteAssetsBatchRequest {
    #[serde(rename = "noteIds")]
    pub note_ids: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct ExecuteShellScriptRequest {
    #[serde(rename = "scriptContent")]
    pub script_content: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u64,
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct DiscoverAgentSessionRequest {
    #[serde(rename = "agentType")]
    pub agent_type: String,
    pub cwd: String,
    #[serde(rename = "claimedIds")]
    pub claimed_ids: Vec<String>,
    #[serde(rename = "agentPid")]
    pub agent_pid: Option<u32>,
    #[serde(rename = "envOverrides")]
    pub env_overrides: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub(super) struct ClaudeProjectDirRequest {
    pub cwd: String,
    #[serde(rename = "claudeConfigDir")]
    pub claude_config_dir: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct OpenInCustomRequest {
    pub executable: String,
    pub args: Vec<String>,
    pub ctx: crate::agent::LaunchContext,
}

#[derive(Deserialize)]
pub(super) struct GenerateValueRequest {
    pub request: crate::generators::GeneratorRequest,
}

#[derive(Deserialize)]
pub(super) struct SetProjectMcpUpstreamsRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "upstreamNames")]
    pub upstream_names: Option<Vec<String>>,
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
pub(super) struct ReversePatchRequest {
    pub path: String,
    pub patch: String,
    pub scope: Option<String>,
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

// --- GitHub poller ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartPollingRequest {
    pub paths: Vec<String>,
    pub issue_filter: String,
    /// Mirrors the `pr_hide_drafts` arg of the `github_start_polling` Tauri
    /// command so the HTTP route reaches parity. Optional for older clients.
    #[serde(default)]
    pub pr_hide_drafts: bool,
}

#[derive(Deserialize)]
pub(super) struct SetVisibilityRequest {
    pub visible: bool,
}

#[derive(Deserialize)]
pub(super) struct PollRepoRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub(super) struct UpdatePathsRequest {
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct SetIssueFilterRequest {
    pub filter: String,
}

#[derive(Deserialize)]
pub(super) struct SetApiDebugRequest {
    pub enabled: bool,
}

/// Body of `POST /diagnostics/capture`. `session_id` narrows the tap to one
/// session; omitted, every session is recorded.
#[derive(Deserialize)]
pub(super) struct SetCaptureRequest {
    pub enabled: bool,
    #[serde(default)]
    pub session_id: Option<String>,
}

// --- Terminal grid command types ---

#[derive(Deserialize)]
pub(super) struct TerminalScrollRequest {
    pub delta: i32,
}

/// The resolved terminal theme, for answering OSC 10/11/12 colour queries.
#[derive(Deserialize)]
pub(super) struct TerminalThemeColorsRequest {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
}

#[derive(Deserialize)]
pub(super) struct TerminalScrollToRequest {
    pub line: usize,
}

#[derive(Deserialize)]
pub(super) struct TerminalScrollToAnchorRequest {
    pub anchor: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct TerminalScrollToOffsetRequest {
    pub offset: usize,
}

#[derive(Deserialize)]
pub(super) struct TerminalSearchRequest {
    pub query: String,
}

#[derive(Deserialize)]
pub(super) struct TerminalRowQuery {
    pub row: usize,
}

#[derive(Deserialize)]
pub(super) struct TerminalLinesQuery {
    pub start: usize,
    pub end: usize,
}

#[derive(Deserialize)]
pub(super) struct ChatViewQuery {
    pub epoch: Option<u64>,
    pub from_seq: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct TerminalStyledRowsQuery {
    pub start: usize,
    pub count: usize,
}

#[derive(Deserialize)]
pub(super) struct TerminalCellQuery {
    pub row: usize,
    pub col: usize,
}

#[derive(Deserialize)]
pub(super) struct TerminalSelectionQuery {
    #[serde(rename = "startRow")]
    pub start_row: usize,
    #[serde(rename = "startCol")]
    pub start_col: usize,
    #[serde(rename = "endRow")]
    pub end_row: usize,
    #[serde(rename = "endCol")]
    pub end_col: usize,
    #[serde(rename = "historyBase")]
    pub history_base: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct SessionVisibleRequest {
    pub visible: bool,
}

#[derive(Deserialize)]
pub(super) struct ClaudeTimelineQuery {
    pub scope: String,
    pub days: Option<u32>,
}

#[derive(Deserialize)]
pub(super) struct ClaudeStatsQuery {
    pub scope: String,
}

// --- Git panel (story 064) ---

#[derive(Deserialize)]
pub(super) struct GitGutterQuery {
    pub path: String,
    pub file: String,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct GitRecentBranchesQuery {
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct GitBranchBaseQuery {
    pub path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
}

#[derive(Deserialize)]
pub(super) struct GitWorktreeDirtyQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
}

#[derive(Deserialize)]
pub(super) struct GitRepoQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct GitCommitGraphQuery {
    pub path: String,
    pub count: Option<u32>,
}

#[derive(Deserialize)]
pub(super) struct GitCloneBranchNameRequest {
    #[serde(rename = "sourceBranch")]
    pub source_branch: String,
    #[serde(rename = "existingNames")]
    pub existing_names: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct GitCreateBranchRequest {
    pub path: String,
    pub name: String,
    #[serde(rename = "startPoint")]
    pub start_point: Option<String>,
    pub checkout: bool,
}

#[derive(Deserialize)]
pub(super) struct GitDeleteBranchRequest {
    pub path: String,
    pub name: String,
    pub force: bool,
}

#[derive(Deserialize)]
pub(super) struct GitDeleteLocalBranchRequest {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    /// Checkout holding the ref. Distinct from `branch_name`: the branch is what
    /// gets deleted, the workspace is what gets disposed of (#726-5ac7).
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
    #[serde(rename = "keepWorktree")]
    pub keep_worktree: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct GitUpdateFromBaseRequest {
    pub path: String,
    #[serde(rename = "branchName")]
    pub branch_name: String,
    pub strategy: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RemoteConnectionPasswordRequest {
    pub password: String,
}

#[derive(Deserialize)]
pub(super) struct RemoteConnectionRelayPairingRequest {
    pub bundle: String,
}

#[derive(Deserialize)]
pub(super) struct RemoteConnectionTokenRequest {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub username: String,
}
