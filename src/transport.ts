/**
 * Transport abstraction layer — auto-detects Tauri IPC vs HTTP/WebSocket.
 *
 * In Tauri mode: uses invoke() for RPC, listen() for events.
 * In browser mode: uses fetch() for RPC, WebSocket for PTY streaming.
 */

import { appLogger } from "./stores/appLogger";
import type { LogLine } from "./mobile/utils/logLine";

// ---------------------------------------------------------------------------
// MCP upstream config types (mirrors Rust structs in mcp_upstream_config.rs)
// ---------------------------------------------------------------------------

export type UpstreamTransport =
  | { type: "http"; url: string }
  | { type: "stdio"; command: string; args?: string[]; env?: Record<string, string> };

export type FilterMode = "allow" | "deny";

export interface ToolFilter {
  mode: FilterMode;
  patterns: string[];
}

export interface UpstreamMcpServer {
  id: string;
  name: string;
  transport: UpstreamTransport;
  enabled: boolean;
  timeout_secs: number;
  tool_filter?: ToolFilter;
}

export interface UpstreamMcpConfig {
  servers: UpstreamMcpServer[];
}

// ---------------------------------------------------------------------------

/** Detect whether we're running inside a Tauri webview */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in globalThis;
}

/** HTTP method + path mapping for a Tauri command */
export interface HttpMapping {
  method: "GET" | "POST" | "PUT" | "DELETE";
  path: string;
  body?: unknown;
  /** Transform the HTTP response before returning (e.g. for can_spawn_session) */
  transform?: (data: unknown) => unknown;
}

/** Helper to encode a required argument for URL path/query usage */
function encodeArg(command: string, args: Record<string, unknown>, key: string): string {
  const val = args[key];
  if (val === undefined || val === null) {
    throw new Error(`mapCommandToHttp(${command}): missing required argument "${key}"`);
  }
  return encodeURIComponent(String(val));
}

/** Args accessor + URL encoder, bound to a specific command invocation */
type ArgEncoder = (key: string) => string;

/** A command table entry: either a mapper function or a browser-unsupported marker */
type CommandTableEntry =
  | { map: (args: Record<string, unknown>, p: ArgEncoder) => HttpMapping }
  | { browserUnsupported: true };

/**
 * Table-driven mapping from Tauri command names to HTTP method/path/body.
 *
 * Each entry is either:
 * - `{ map: (args, p) => HttpMapping }` — a mapper that builds the HTTP request
 * - `{ browserUnsupported: true }` — command requires native OS features
 *
 * The `p` helper encodes a required argument for URL usage (throws if missing).
 */
const COMMAND_TABLE: Record<string, CommandTableEntry> = {
  // --- Browser-unsupported: Dictation (requires local audio hardware) ---
  get_dictation_status: { browserUnsupported: true },
  get_model_info: { browserUnsupported: true },
  download_whisper_model: { browserUnsupported: true },
  delete_whisper_model: { browserUnsupported: true },
  start_dictation: { browserUnsupported: true },
  stop_dictation_and_transcribe: { browserUnsupported: true },
  get_correction_map: { browserUnsupported: true },
  set_correction_map: { browserUnsupported: true },
  list_audio_devices: { browserUnsupported: true },
  inject_text: { browserUnsupported: true },
  get_dictation_config: { browserUnsupported: true },
  set_dictation_config: { browserUnsupported: true },
  // --- Browser-unsupported: OS integration ---
  open_in_app: { browserUnsupported: true },
  // --- Browser-unsupported: Native audio ---
  play_notification_sound: { browserUnsupported: true },
  // --- Browser-unsupported: Relay (desktop config only) ---
  get_relay_status: { browserUnsupported: true },
  // --- Browser-unsupported: Update channel check (hardcoded GitHub URLs) ---
  check_update_channel: { browserUnsupported: true },
  // --- Browser-unsupported: Agent session verification (local filesystem) ---
  verify_agent_session: { browserUnsupported: true },
  // --- Browser-unsupported: MCP upstream (uses OS keyring) ---
  load_mcp_upstreams: { browserUnsupported: true },
  save_mcp_upstreams: { browserUnsupported: true },
  reconnect_mcp_upstream: { browserUnsupported: true },
  save_mcp_upstream_credential: { browserUnsupported: true },
  delete_mcp_upstream_credential: { browserUnsupported: true },

  // --- Session lifecycle ---
  create_pty: {
    map: (args) => ({
      method: "POST",
      path: "/sessions",
      body: args.config as Record<string, unknown>,
      transform: (data: unknown) => (data as { session_id: string }).session_id,
    }),
  },
  create_pty_with_worktree: {
    map: (args) => ({
      method: "POST",
      path: "/sessions/worktree",
      body: { config: args.config, base_repo: args.baseRepo, branch_name: args.branchName },
    }),
  },
  write_pty: {
    map: (args) => ({
      method: "POST",
      path: `/sessions/${args.sessionId ?? args.id}/write`,
      body: { data: args.data },
    }),
  },
  set_session_name: {
    map: (args) => ({
      method: "PUT",
      path: `/sessions/${args.sessionId}/name`,
      body: { name: args.name },
    }),
  },
  resize_pty: {
    map: (args) => ({
      method: "POST",
      path: `/sessions/${args.sessionId}/resize`,
      body: { rows: args.rows, cols: args.cols },
    }),
  },
  pause_pty: {
    map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/pause` }),
  },
  resume_pty: {
    map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/resume` }),
  },
  get_kitty_flags: {
    map: (args) => ({ method: "GET", path: `/sessions/${args.sessionId}/kitty-flags` }),
  },
  close_pty: {
    map: (args) => ({ method: "DELETE", path: `/sessions/${args.sessionId}` }),
  },
  get_session_foreground_process: {
    map: (args) => ({
      method: "GET",
      path: `/sessions/${args.sessionId}/foreground`,
      transform: (data) => (data as { agent: string | null }).agent,
    }),
  },

  // --- Orchestrator ---
  get_orchestrator_stats: { map: () => ({ method: "GET", path: "/stats" }) },
  get_session_metrics: { map: () => ({ method: "GET", path: "/metrics" }) },
  list_active_sessions: { map: () => ({ method: "GET", path: "/sessions" }) },
  can_spawn_session: {
    map: () => ({
      method: "GET",
      path: "/stats",
      transform: (data) => {
        const stats = data as { active_sessions: number; max_sessions: number };
        return stats.active_sessions < stats.max_sessions;
      },
    }),
  },

  // --- Config: app ---
  load_config: { map: () => ({ method: "GET", path: "/config" }) },
  save_config: { map: (args) => ({ method: "PUT", path: "/config", body: args.config }) },
  hash_password: {
    map: (args) => ({
      method: "POST",
      path: "/config/hash-password",
      body: { password: args.password },
      transform: (data) => (data as { hash: string }).hash,
    }),
  },

  // --- Config: notifications ---
  load_notification_config: { map: () => ({ method: "GET", path: "/config/notifications" }) },
  save_notification_config: {
    map: (args) => ({ method: "PUT", path: "/config/notifications", body: args.config }),
  },

  // --- Config: UI prefs ---
  load_ui_prefs: { map: () => ({ method: "GET", path: "/config/ui-prefs" }) },
  save_ui_prefs: {
    map: (args) => ({ method: "PUT", path: "/config/ui-prefs", body: args.config }),
  },

  // --- Config: repo settings ---
  load_repo_settings: { map: () => ({ method: "GET", path: "/config/repo-settings" }) },
  save_repo_settings: {
    map: (args) => ({ method: "PUT", path: "/config/repo-settings", body: args.config }),
  },
  check_has_custom_settings: {
    map: (_args, p) => ({ method: "GET", path: `/config/repo-settings/has-custom?path=${p("path")}` }),
  },
  load_repo_defaults: { map: () => ({ method: "GET", path: "/config/repo-defaults" }) },
  save_repo_defaults: {
    map: (args) => ({ method: "PUT", path: "/config/repo-defaults", body: args.config }),
  },

  // --- Config: repositories ---
  load_repositories: { map: () => ({ method: "GET", path: "/config/repositories" }) },
  save_repositories: {
    map: (args) => ({ method: "PUT", path: "/config/repositories", body: args.config }),
  },

  // --- Config: prompt library ---
  load_prompt_library: { map: () => ({ method: "GET", path: "/config/prompt-library" }) },
  save_prompt_library: {
    map: (args) => ({ method: "PUT", path: "/config/prompt-library", body: args.config }),
  },

  // --- Git/GitHub ---
  get_repo_info: {
    map: (_args, p) => ({ method: "GET", path: `/repo/info?path=${p("path")}` }),
  },
  get_git_diff: {
    map: (_args, p) => ({
      method: "GET",
      path: `/repo/diff?path=${p("path")}`,
      transform: (data) => (data as { diff: string }).diff,
    }),
  },
  get_diff_stats: {
    map: (_args, p) => ({ method: "GET", path: `/repo/diff-stats?path=${p("path")}` }),
  },
  get_changed_files: {
    map: (_args, p) => ({ method: "GET", path: `/repo/files?path=${p("path")}` }),
  },
  get_file_diff: {
    map: (args, p) => {
      let diffUrl = `/repo/file-diff?path=${p("path")}&file=${p("file")}`;
      if (args?.scope) diffUrl += `&scope=${encodeURIComponent(String(args.scope))}`;
      if (args?.untracked) diffUrl += `&untracked=true`;
      return { method: "GET", path: diffUrl };
    },
  },
  get_github_status: {
    map: (_args, p) => ({ method: "GET", path: `/repo/github?path=${p("path")}` }),
  },
  get_repo_pr_statuses: {
    map: (_args, p) => ({ method: "GET", path: `/repo/prs?path=${p("path")}` }),
  },
  get_all_pr_statuses: {
    map: (args) => ({
      method: "POST",
      path: "/repo/prs/batch",
      body: { paths: args.paths, include_merged: args.includeMerged },
    }),
  },
  get_git_branches: {
    map: (_args, p) => ({ method: "GET", path: `/repo/branches?path=${p("path")}` }),
  },
  get_merged_branches: {
    map: (_args, p) => ({ method: "GET", path: `/repo/branches/merged?path=${p("repoPath")}` }),
  },
  get_repo_summary: {
    map: (_args, p) => ({ method: "GET", path: `/repo/summary?path=${p("repoPath")}` }),
  },
  get_repo_structure: {
    map: (_args, p) => ({ method: "GET", path: `/repo/structure?path=${p("repoPath")}` }),
  },
  get_repo_diff_stats: {
    map: (_args, p) => ({ method: "GET", path: `/repo/diff-stats/batch?path=${p("repoPath")}` }),
  },
  get_ci_checks: {
    map: (_args, p) => ({ method: "GET", path: `/repo/ci?path=${p("path")}&pr_number=${p("prNumber")}` }),
  },
  rename_branch: {
    map: (args) => ({
      method: "POST",
      path: "/repo/branch/rename",
      body: { path: args.path, old_name: args.oldName, new_name: args.newName },
    }),
  },
  get_initials: {
    map: (_args, p) => ({ method: "GET", path: `/repo/initials?name=${p("name")}` }),
  },
  check_is_main_branch: {
    map: (_args, p) => ({ method: "GET", path: `/repo/is-main-branch?branch=${p("branch")}` }),
  },
  get_remote_url: {
    map: (_args, p) => ({ method: "GET", path: `/repo/remote-url?path=${p("path")}` }),
  },
  get_git_panel_context: {
    map: (_args, p) => ({ method: "GET", path: `/repo/panel-context?path=${p("path")}` }),
  },
  run_git_command: {
    map: (args) => ({
      method: "POST",
      path: "/repo/run-git",
      body: { path: args.path, args: args.args },
    }),
  },
  get_working_tree_status: {
    map: (_args, p) => ({ method: "GET", path: `/repo/working-tree-status?path=${p("path")}` }),
  },
  git_stage_files: {
    map: (args) => ({
      method: "POST",
      path: "/repo/stage",
      body: { path: args.path, files: args.files },
    }),
  },
  git_unstage_files: {
    map: (args) => ({
      method: "POST",
      path: "/repo/unstage",
      body: { path: args.path, files: args.files },
    }),
  },
  git_discard_files: {
    map: (args) => ({
      method: "POST",
      path: "/repo/discard",
      body: { path: args.path, files: args.files },
    }),
  },
  git_commit: {
    map: (args) => ({
      method: "POST",
      path: "/repo/commit",
      body: { path: args.path, message: args.message, amend: args.amend },
    }),
  },
  get_commit_log: {
    map: (args, p) => {
      let url = `/repo/commit-log?path=${p("path")}`;
      if (args.count != null) url += `&count=${args.count}`;
      if (args.after) url += `&after=${encodeURIComponent(String(args.after))}`;
      return { method: "GET", path: url };
    },
  },
  get_stash_list: {
    map: (_args, p) => ({ method: "GET", path: `/repo/stash?path=${p("path")}` }),
  },
  git_stash_apply: {
    map: (args) => ({
      method: "POST",
      path: "/repo/stash/apply",
      body: { path: args.path, stash_ref: args.stashRef },
    }),
  },
  git_stash_pop: {
    map: (args) => ({
      method: "POST",
      path: "/repo/stash/pop",
      body: { path: args.path, stash_ref: args.stashRef },
    }),
  },
  git_stash_drop: {
    map: (args) => ({
      method: "POST",
      path: "/repo/stash/drop",
      body: { path: args.path, stash_ref: args.stashRef },
    }),
  },
  git_stash_show: {
    map: (_args, p) => ({
      method: "GET",
      path: `/repo/stash/show?path=${p("path")}&stash_ref=${p("stashRef")}`,
    }),
  },
  get_file_history: {
    map: (args, p) => {
      let url = `/repo/file-history?path=${p("path")}&file=${p("file")}`;
      if (args.count != null) url += `&count=${args.count}`;
      if (args.after) url += `&after=${encodeURIComponent(String(args.after))}`;
      return { method: "GET", path: url };
    },
  },
  get_file_blame: {
    map: (_args, p) => ({
      method: "GET",
      path: `/repo/file-blame?path=${p("path")}&file=${p("file")}`,
    }),
  },

  // --- Worktrees ---
  list_worktrees: { map: () => ({ method: "GET", path: "/worktrees" }) },
  get_worktrees_dir: {
    map: (args) => {
      const rp = args?.repoPath as string | undefined;
      return {
        method: "GET",
        path: rp ? `/worktrees/dir?repo_path=${encodeURIComponent(rp)}` : "/worktrees/dir",
        transform: (data) => (data as { dir: string }).dir,
      };
    },
  },
  get_worktree_paths: {
    map: (_args, p) => ({ method: "GET", path: `/worktrees/paths?path=${p("repoPath")}` }),
  },
  create_worktree: {
    map: (args) => ({
      method: "POST",
      path: "/worktrees",
      body: { base_repo: args.baseRepo, branch_name: args.branchName, base_ref: args.baseRef },
    }),
  },
  remove_worktree: {
    map: (args, p) => ({
      method: "DELETE",
      path: `/worktrees/${p("branchName")}?repoPath=${p("repoPath")}&deleteBranch=${args.deleteBranch ?? true}`,
    }),
  },
  generate_worktree_name_cmd: {
    map: (args) => ({
      method: "POST",
      path: "/worktrees/generate-name",
      body: { existing_names: args.existingNames },
    }),
  },
  finalize_merged_worktree: {
    map: (args) => ({
      method: "POST",
      path: "/worktrees/finalize",
      body: { repoPath: args.repoPath, branchName: args.branchName, action: args.action },
    }),
  },
  checkout_remote_branch: {
    map: (args) => ({
      method: "POST",
      path: "/repo/checkout-remote",
      body: { repoPath: args.repoPath, branchName: args.branchName },
    }),
  },
  detect_orphan_worktrees: {
    map: (_args, p) => ({ method: "GET", path: `/repo/orphan-worktrees?repoPath=${p("repoPath")}` }),
  },
  remove_orphan_worktree: {
    map: (args) => ({
      method: "POST",
      path: "/repo/remove-orphan",
      body: { repoPath: args.repoPath, worktreePath: args.worktreePath },
    }),
  },
  run_setup_script: {
    map: (args) => ({
      method: "POST",
      path: "/worktrees/run-script",
      body: { script: args.script, cwd: args.cwd },
    }),
  },
  merge_pr_via_github: {
    map: (args) => ({
      method: "POST",
      path: "/repo/merge-pr",
      body: { repoPath: args.repoPath, prNumber: args.prNumber, mergeMethod: args.mergeMethod },
    }),
  },
  get_pr_diff: {
    map: (args, p) => ({
      method: "GET",
      path: `/repo/pr-diff?path=${p("repoPath")}&pr=${args.prNumber}`,
    }),
  },
  approve_pr: {
    map: (args) => ({
      method: "POST",
      path: "/repo/approve-pr",
      body: { repoPath: args.repoPath, prNumber: args.prNumber },
    }),
  },
  list_local_branches: {
    map: (_args, p) => ({ method: "GET", path: `/repo/local-branches?path=${p("repoPath")}` }),
  },

  // --- File operations ---
  list_markdown_files: {
    map: (_args, p) => ({ method: "GET", path: `/repo/markdown-files?path=${p("path")}` }),
  },
  read_file: {
    map: (_args, p) => ({ method: "GET", path: `/repo/file?path=${p("path")}&file=${p("file")}` }),
  },

  // --- Prompt processing ---
  process_prompt_content: {
    map: (args) => ({
      method: "POST",
      path: "/prompt/process",
      body: { content: args.content, variables: args.variables },
    }),
  },
  extract_prompt_variables: {
    map: (args) => ({
      method: "POST",
      path: "/prompt/extract-variables",
      body: { content: args.content },
    }),
  },

  // --- Agents ---
  detect_agents: { map: () => ({ method: "GET", path: "/agents" }) },
  detect_agent_binary: {
    map: (_args, p) => ({ method: "GET", path: `/agents/detect?binary=${p("binary")}` }),
  },
  detect_installed_ides: { map: () => ({ method: "GET", path: "/agents/ides" }) },

  // --- Watchers ---
  start_head_watcher: {
    map: (_args, p) => ({ method: "POST", path: `/watchers/head?path=${p("repoPath")}` }),
  },
  stop_head_watcher: {
    map: (_args, p) => ({ method: "DELETE", path: `/watchers/head?path=${p("repoPath")}` }),
  },
  start_repo_watcher: {
    map: (_args, p) => ({ method: "POST", path: `/watchers/repo?path=${p("repoPath")}` }),
  },
  stop_repo_watcher: {
    map: (_args, p) => ({ method: "DELETE", path: `/watchers/repo?path=${p("repoPath")}` }),
  },
  start_dir_watcher: {
    map: (_args, p) => ({ method: "POST", path: `/watchers/dir?path=${p("path")}` }),
  },
  stop_dir_watcher: {
    map: (_args, p) => ({ method: "DELETE", path: `/watchers/dir?path=${p("path")}` }),
  },

  // --- MCP status ---
  get_mcp_status: { map: () => ({ method: "GET", path: "/mcp/status" }) },

  // --- Network ---
  get_local_ip: { map: () => ({ method: "GET", path: "/system/local-ip" }) },
  get_local_ips: { map: () => ({ method: "GET", path: "/system/local-ips" }) },

  // --- File browser ---
  list_directory: {
    map: (_args, p) => ({ method: "GET", path: `/fs/list?repoPath=${p("repoPath")}&subdir=${p("subdir")}` }),
  },
  fs_read_file: {
    map: (_args, p) => ({ method: "GET", path: `/fs/read?repoPath=${p("repoPath")}&file=${p("file")}` }),
  },
  read_external_file: {
    map: (_args, p) => ({ method: "GET", path: `/fs/read-external?path=${p("path")}` }),
  },
  write_file: {
    map: (args) => ({
      method: "POST",
      path: "/fs/write",
      body: { repoPath: args.repoPath, file: args.file, content: args.content },
    }),
  },
  create_directory: {
    map: (args) => ({
      method: "POST",
      path: "/fs/mkdir",
      body: { repoPath: args.repoPath, dir: args.dir },
    }),
  },
  delete_path: {
    map: (args) => ({
      method: "POST",
      path: "/fs/delete",
      body: { repoPath: args.repoPath, path: args.path },
    }),
  },
  rename_path: {
    map: (args) => ({
      method: "POST",
      path: "/fs/rename",
      body: { repoPath: args.repoPath, from: args.from, to: args.to },
    }),
  },
  copy_path: {
    map: (args) => ({
      method: "POST",
      path: "/fs/copy",
      body: { repoPath: args.repoPath, from: args.from, to: args.to },
    }),
  },
  add_to_gitignore: {
    map: (args) => ({
      method: "POST",
      path: "/fs/gitignore",
      body: { repoPath: args.repoPath, pattern: args.pattern },
    }),
  },
  search_content: {
    map: (args, p) => {
      let path = `/fs/search-content?repoPath=${p("repoPath")}&query=${p("query")}&caseSensitive=${p("caseSensitive")}&useRegex=${p("useRegex")}&wholeWord=${p("wholeWord")}`;
      if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
      return { method: "GET", path };
    },
  },

  // --- Notes ---
  load_notes: { map: () => ({ method: "GET", path: "/config/notes" }) },
  save_notes: { map: (args) => ({ method: "PUT", path: "/config/notes", body: args.config }) },

  // --- Recent commits ---
  get_recent_commits: {
    map: (args, p) => ({
      method: "GET",
      path: `/repo/recent-commits?path=${p("path")}&count=${args.count ?? 5}`,
    }),
  },

  // --- Plugins ---
  list_user_plugins: { map: () => ({ method: "GET", path: "/plugins/list" }) },

  // --- App Logger ---
  push_log: {
    map: (args) => ({
      method: "POST",
      path: "/logs",
      body: { level: args.level, source: args.source, message: args.message, data_json: args.dataJson },
    }),
  },
  get_logs: {
    map: (args) => ({ method: "GET", path: `/logs?limit=${args.limit ?? 0}` }),
  },
  clear_logs: { map: () => ({ method: "DELETE", path: "/logs" }) },
};

/** Map a Tauri invoke command + args to an HTTP method/path/body */
export function mapCommandToHttp(command: string, args: Record<string, unknown>): HttpMapping {
  const entry = COMMAND_TABLE[command];
  if (!entry) {
    throw new Error(`No HTTP mapping for command: ${command}`);
  }
  if ("browserUnsupported" in entry) {
    throw new Error(
      `Command "${command}" requires native OS features and is not available in browser mode`,
    );
  }
  const p: ArgEncoder = (key) => encodeArg(command, args, key);
  return entry.map(args, p);
}

/** Build a full URL for HTTP transport using current window origin */
export function buildHttpUrl(path: string): string {
  if (typeof window !== "undefined" && window.location?.origin) {
    return `${window.location.origin}${path}`;
  }
  return path;
}

/**
 * In-flight deduplication for idempotent (GET) RPC calls.
 * Concurrent identical calls share the same Promise — cleared on settle.
 */
const _inflight = new Map<string, Promise<unknown>>();

/** True if the command maps to an HTTP GET (idempotent, safe to deduplicate). */
function isIdempotentRpc(command: string, args: Record<string, unknown>): boolean {
  try {
    return mapCommandToHttp(command, args).method === "GET";
  } catch {
    return false;
  }
}

/**
 * RPC call — uses Tauri invoke() or HTTP fetch() based on environment.
 * Concurrent identical idempotent calls are coalesced into a single in-flight request.
 * Usage: `const result = await rpc<string>("create_pty", { config });`
 */
export function rpc<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isIdempotentRpc(command, args)) {
    const key = `${command}:${JSON.stringify(args)}`;
    const existing = _inflight.get(key) as Promise<T> | undefined;
    if (existing) return existing;
    const promise = rpcImpl<T>(command, args).finally(() => _inflight.delete(key));
    _inflight.set(key, promise as Promise<unknown>);
    return promise;
  }
  return rpcImpl<T>(command, args);
}

async function rpcImpl<T>(command: string, args: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    // Only pass args if non-empty (matches Tauri invoke signature)
    if (Object.keys(args).length > 0) {
      return invoke<T>(command, args);
    }
    return invoke<T>(command);
  }

  const mapping = mapCommandToHttp(command, args);
  const url = buildHttpUrl(mapping.path);

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 30_000);

  const init: RequestInit = {
    method: mapping.method,
    headers: { "Content-Type": "application/json" },
    signal: controller.signal,
  };
  if (mapping.body !== undefined) {
    init.body = JSON.stringify(mapping.body);
  }

  let resp: Response;
  try {
    resp = await fetch(url, init);
  } finally {
    clearTimeout(timeoutId);
  }
  if (!resp.ok) {
    const text = await resp.text().catch(() => resp.statusText);
    throw new Error(`RPC ${command} failed: ${resp.status} ${text}`);
  }

  const contentType = resp.headers.get("content-type") || "";
  let data: unknown;
  if (contentType.includes("application/json")) {
    data = await resp.json();
  } else {
    const text = await resp.text();
    // Try parsing as JSON anyway (some endpoints may not set content-type)
    try {
      data = JSON.parse(text);
    } catch {
      data = text;
    }
  }

  if (mapping.transform) {
    return mapping.transform(data) as T;
  }
  if (data === null || data === undefined) {
    throw new Error(`RPC ${command}: empty response body`);
  }
  return data as T;
}

/** Unsubscribe function returned by subscribe() */
export type Unsubscribe = () => void;

/** Parsed event from WebSocket JSON framing */
export interface WsParsedEvent {
  type: string;
  [key: string]: unknown;
}

/**
 * Subscribe to PTY session events.
 *
 * In Tauri: uses listen() for pty-output-{sessionId}, pty-exit-{sessionId}.
 * In browser: uses WebSocket to /sessions/{sessionId}/stream with JSON framing:
 *   - {"type":"output","data":"..."} for raw PTY output
 *   - {"type":"parsed","event":{...}} for structured events (questions, rate limits)
 *   - {"type":"exit"} / {"type":"closed"} for session lifecycle
 *
 * @param sessionId - PTY session ID
 * @param onData - Called with each chunk of PTY output
 * @param onExit - Called when the session exits
 * @param onParsed - Optional: called with structured parsed events (browser mode)
 * @returns Promise resolving to an unsubscribe function
 */
export interface SubscribePtyOptions {
  /** Request ANSI-stripped plain text from the server (for non-terminal views like mobile) */
  stripAnsi?: boolean;
  /**
   * Use VT100-extracted log lines (`format=log`).
   * When `onLogLines` is set, structured LogLine objects are delivered there.
   * Otherwise `onData` is called with `\n`-joined plain text (backward compat).
   * Overrides `stripAnsi` when set.
   */
  format?: "log";
  /**
   * Receive structured LogLine objects from `format=log` frames.
   * Each LogLine has `spans: [{text, fg?, bg?, bold?, italic?, underline?}]`.
   */
  onLogLines?: (lines: LogLine[]) => void;
  /** Receive current screen rows (LogLine objects with styled spans) pushed alongside log frames. */
  onScreenRows?: (rows: unknown[]) => void;
  /** Receive the current PTY input line text (extracted from prompt row). */
  onInputLine?: (text: string | null) => void;
  /** Starting offset for log-mode catch-up (skip lines already fetched via HTTP). */
  logOffset?: number;
  /** Receive real-time SessionState snapshots pushed by the server on parsed events. */
  onStateChange?: (state: Record<string, unknown>) => void;
  onParsed?: (event: WsParsedEvent) => void;
}

export async function subscribePty(
  sessionId: string,
  onData: (data: string) => void,
  onExit: () => void,
  onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions,
): Promise<Unsubscribe> {
  // Normalize overloaded 4th param: function (legacy) or options object
  const opts: SubscribePtyOptions = typeof onParsedOrOptions === "function"
    ? { onParsed: onParsedOrOptions }
    : onParsedOrOptions ?? {};
  const onParsed = opts.onParsed;
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    const unlistenOutput = await listen<{ data: string }>(`pty-output-${sessionId}`, (event) => {
      onData(event.payload.data);
    });
    const unlistenExit = await listen(`pty-exit-${sessionId}`, () => {
      onExit();
    });
    return () => {
      unlistenOutput();
      unlistenExit();
    };
  }

  // Browser mode: WebSocket with JSON framing
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const queryFormat = opts.format === "log" ? "log" : opts.stripAnsi ? "text" : null;
  const params = new URLSearchParams();
  if (queryFormat) params.set("format", queryFormat);
  if (opts.logOffset != null) params.set("offset", String(opts.logOffset));
  const query = params.size > 0 ? `?${params}` : "";
  const wsUrl = `${protocol}//${window.location.host}/sessions/${sessionId}/stream${query}`;
  const ws = new WebSocket(wsUrl);

  const handleMessage = (event: MessageEvent) => {
    const raw = event.data as string;
    // JSON frame detection: starts with { and contains "type"
    if (raw.startsWith("{")) {
      try {
        const frame = JSON.parse(raw) as WsParsedEvent;
        switch (frame.type) {
          case "output":
            onData(frame.data as string);
            break;
          case "log": {
            const lines = frame.lines as LogLine[] | undefined;
            if (lines && lines.length > 0) {
              if (opts.onLogLines) {
                opts.onLogLines(lines);
              } else {
                // Backward compat: join span texts as plain string
                const texts = lines.map((l) => {
                  if (typeof l === "string") return l;
                  if (l && typeof l === "object" && "spans" in l) {
                    return ((l as { spans: { text: string }[] }).spans || [])
                      .map((s) => s.text).join("");
                  }
                  return String(l);
                });
                onData(texts.join("\n"));
              }
            }
            const screen = frame.screen as unknown[] | undefined;
            if (screen && opts.onScreenRows) {
              opts.onScreenRows(screen);
            }
            if (opts.onInputLine && frame.screen !== undefined) {
              const il = (frame as Record<string, unknown>).input_line;
              opts.onInputLine(typeof il === "string" ? il : null);
            }
            break;
          }
          case "state":
            if (opts.onStateChange && frame.state) {
              opts.onStateChange(frame.state as Record<string, unknown>);
            }
            break;
          case "parsed":
            onParsed?.(frame);
            break;
          case "exit":
          case "closed":
            onExit();
            break;
        }
        return;
      } catch {
        // Not valid JSON — treat as raw output (backward compat)
      }
    }
    onData(raw);
  };

  // Wait for connection to open. Wire all handlers inside the promise so
  // a pre-handshake close (e.g. session not found) correctly rejects.
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = (e) => reject(new Error(`WebSocket connection failed: ${e}`));
    ws.onclose = (event: CloseEvent) => {
      reject(new Error(`WebSocket closed before opening (code ${event.code}): ${event.reason || "no reason"}`));
    };
    ws.onmessage = handleMessage;
  });

  // Re-wire onclose for the live session after successful open
  ws.onclose = (event: CloseEvent) => {
    if (!event.wasClean) {
      appLogger.debug("network", `WebSocket closed abnormally (code ${event.code}): ${event.reason || "unknown"}`);
    }
    onExit();
  };

  return () => {
    ws.close();
  };
}

/**
 * Subscribe to application-level events (head-changed, repo-changed, etc.)
 *
 * In Tauri: delegates to individual listen() calls.
 * In browser: creates a single EventSource to /events SSE endpoint.
 *
 * @param handlers - Map of event type → callback
 * @returns Promise resolving to an unsubscribe function
 */
export async function subscribeEvents(
  handlers: Record<string, (payload: unknown) => void>,
): Promise<Unsubscribe> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    const unsubscribers: Array<() => void> = [];
    for (const [eventType, handler] of Object.entries(handlers)) {
      const unlisten = await listen(eventType, (event) => handler(event.payload));
      unsubscribers.push(unlisten);
    }
    return () => unsubscribers.forEach((fn) => fn());
  }

  // Browser mode: SSE via EventSource
  const types = Object.keys(handlers).join(",");
  const url = buildHttpUrl(`/events?types=${encodeURIComponent(types)}`);
  const es = new EventSource(url);

  for (const [eventType, handler] of Object.entries(handlers)) {
    es.addEventListener(eventType, ((event: MessageEvent) => {
      try {
        const payload = JSON.parse(event.data);
        handler(payload);
      } catch {
        appLogger.warn("network", `Failed to parse SSE event "${eventType}": ${event.data}`);
      }
    }) as EventListener);
  }

  es.addEventListener("lagged", ((event: MessageEvent) => {
    appLogger.warn("network", `SSE lagged: ${event.data}`);
  }) as EventListener);

  es.onerror = () => {
    appLogger.debug("network", "SSE connection error — will auto-reconnect");
  };

  return () => es.close();
}
