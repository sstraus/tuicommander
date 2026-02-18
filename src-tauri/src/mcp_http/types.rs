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
pub(super) struct ResizeRequest {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Deserialize)]
pub(super) struct OutputQuery {
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub(super) struct PathQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub(super) struct CiChecksQuery {
    pub path: String,
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
}

#[derive(Deserialize)]
pub(super) struct RemoveWorktreeQuery {
    #[serde(rename = "repoPath")]
    pub repo_path: String,
}

#[derive(Deserialize)]
pub(super) struct GenerateWorktreeNameRequest {
    pub existing_names: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct FileQuery {
    pub path: String,
    pub file: String,
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

#[derive(Deserialize)]
pub(super) struct McpSessionQuery {
    #[serde(rename = "sessionId")]
    pub session_id: String,
}
