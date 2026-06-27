/**
 * Transport abstraction layer — auto-detects Tauri IPC vs HTTP/WebSocket.
 *
 * In Tauri mode: uses invoke() for RPC, listen() for events.
 * In browser mode: uses fetch() for RPC, WebSocket for PTY streaming.
 */

import type { LogLine } from "./mobile/utils/logLine";
import { appLogger } from "./stores/appLogger";
import { remoteConnectionsStore } from "./stores/remoteConnections";

// ---------------------------------------------------------------------------
// MCP upstream config types (mirrors Rust structs in mcp_upstream_config.rs)
// ---------------------------------------------------------------------------

export type UpstreamTransport =
	| { type: "http"; url: string }
	| { type: "stdio"; command: string; args?: string[]; env?: Record<string, string>; cwd?: string };

export type FilterMode = "allow" | "deny";

export interface ToolFilter {
	mode: FilterMode;
	patterns: string[];
}

export type UpstreamAuth =
	| { type: "bearer"; token: string }
	| {
			type: "oauth2";
			client_id: string;
			scopes?: string[];
			authorization_endpoint?: string;
			token_endpoint?: string;
	  };

export interface UpstreamMcpServer {
	id: string;
	name: string;
	transport: UpstreamTransport;
	enabled: boolean;
	timeout_secs: number;
	tool_filter?: ToolFilter;
	auth?: UpstreamAuth;
}

export interface UpstreamMcpConfig {
	servers: UpstreamMcpServer[];
}

// ---------------------------------------------------------------------------

/** Detect whether we're running inside a Tauri webview */
export function isTauri(): boolean {
	return "__TAURI_INTERNALS__" in globalThis && !(globalThis as Record<string, unknown>).__TAURI_SHIM__;
}

/** HTTP method + path mapping for a Tauri command */
export interface HttpMapping {
	method: "GET" | "POST" | "PUT" | "DELETE";
	path: string;
	body?: unknown;
	/** Transform the HTTP response before returning (e.g. for can_spawn_session) */
	transform?: (data: unknown) => unknown;
	/**
	 * Treat an HTTP 404 as a successful `null` result instead of throwing.
	 * Bridges Tauri commands whose contract is `Option<T>` (None → null) onto
	 * REST routes that signal "not found" with 404 (e.g. read_plugin_data).
	 */
	notFoundAsNull?: boolean;
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

/** A command table entry: a mapper function that builds the HTTP request */
type CommandTableEntry = { map: (args: Record<string, unknown>, p: ArgEncoder) => HttpMapping };

/**
 * Table-driven mapping from Tauri command names to HTTP method/path/body.
 *
 * The `p` helper encodes a required argument for URL usage (throws if missing).
 */
const COMMAND_TABLE: Record<string, CommandTableEntry> = {
	// --- Dictation ---
	get_dictation_status: { map: () => ({ method: "GET", path: "/dictation/status" }) },
	get_model_info: { map: () => ({ method: "GET", path: "/dictation/models" }) },
	download_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/download", body: { model: args.model_name } }),
	},
	delete_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/delete", body: { model: args.model_name } }),
	},
	start_dictation: { map: () => ({ method: "POST", path: "/dictation/start" }) },
	stop_dictation_and_transcribe: { map: () => ({ method: "POST", path: "/dictation/stop" }) },
	get_correction_map: { map: () => ({ method: "GET", path: "/dictation/corrections" }) },
	set_correction_map: {
		map: (args) => ({ method: "PUT", path: "/dictation/corrections", body: { map: args.map } }),
	},
	list_audio_devices: { map: () => ({ method: "GET", path: "/dictation/devices" }) },
	inject_text: {
		map: (args) => ({ method: "POST", path: "/dictation/inject", body: { text: args.text } }),
	},
	get_dictation_config: { map: () => ({ method: "GET", path: "/dictation/config" }) },
	set_dictation_config: {
		map: (args) => ({ method: "PUT", path: "/dictation/config", body: args.config }),
	},
	// --- OS integration ---
	open_in_app: {
		map: (args) => ({
			method: "POST",
			path: "/agents/open-in-app",
			body: { path: args.path, app: args.app, line: args.line, col: args.col },
		}),
	},
	// --- Native audio ---
	play_notification_sound: {
		map: (args) => ({
			method: "POST",
			path: "/system/notification-sound",
			body: { sound: args.sound, volume: args.volume },
		}),
	},
	// --- Relay ---
	get_relay_status: { map: () => ({ method: "GET", path: "/system/relay-status" }) },
	// --- Update channel ---
	check_update_channel: {
		map: (_args, p) => ({ method: "GET", path: `/system/check-update?channel=${p("channel")}` }),
	},
	// --- MCP upstream config (proxied through server for keyring access) ---
	load_mcp_upstreams: { map: () => ({ method: "GET", path: "/mcp/upstreams" }) },
	get_mcp_upstream_status: { map: () => ({ method: "GET", path: "/mcp/upstream-status" }) },
	save_mcp_upstreams: {
		map: (args) => ({ method: "PUT", path: "/mcp/upstreams", body: args.config }),
	},
	reconnect_mcp_upstream: {
		map: (args) => ({ method: "POST", path: "/mcp/upstreams/reconnect", body: { name: args.name } }),
	},
	save_mcp_upstream_credential: {
		map: (args) => ({
			method: "POST",
			path: "/mcp/upstreams/credential",
			body: { name: args.name, token: args.token },
		}),
	},
	delete_mcp_upstream_credential: {
		map: (args) => ({ method: "DELETE", path: "/mcp/upstreams/credential", body: { name: args.name } }),
	},

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
		// Browser path: createSessionWithWorktree sends { pty_config, worktree_config };
		// flatten worktree_config into the HTTP route's { config, base_repo, branch_name }.
		map: (args) => {
			const wt = (args.worktree_config ?? {}) as { task_name?: string; base_repo?: string; branch?: string | null };
			return {
				method: "POST",
				path: "/sessions/worktree",
				body: {
					config: args.pty_config,
					base_repo: wt.base_repo,
					branch_name: wt.branch ?? wt.task_name,
				},
			};
		},
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
	get_session_shell_family: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/shell-family`,
		}),
	},
	get_shell_state: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/shell-state`,
			transform: (data) => (data as { state: string | null }).state ?? null,
		}),
	},
	get_last_prompt: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/last-prompt`,
			transform: (data) => (data as { prompt: string | null }).prompt ?? null,
		}),
	},
	get_input_buffer_content: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/input-buffer`,
			transform: (data) => (data as { content: string }).content,
		}),
	},
	get_session_leaf_pid: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/leaf-pid`,
			transform: (data) => (data as { pid: number | null }).pid ?? null,
		}),
	},
	has_foreground_process: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/has-foreground`,
			transform: (data) => (data as { process: string | null }).process ?? null,
		}),
	},
	set_session_visible: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/visible`,
			body: { visible: args.visible },
		}),
	},

	// --- Terminal grid commands ---
	terminal_scroll: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll`,
			body: { delta: args.delta },
		}),
	},
	terminal_scroll_to: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll-to`,
			body: { line: args.line },
		}),
	},
	terminal_scroll_to_offset: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll-to-offset`,
			body: { offset: args.offset },
		}),
	},
	terminal_scroll_info: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/scroll-info`,
		}),
	},
	terminal_search: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/search`,
			body: { query: args.query },
			transform: (data) => (data as { matches: unknown[] }).matches,
		}),
	},
	terminal_search_buffer: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/search-buffer`,
			body: { query: args.query },
			transform: (data) => (data as { matches: unknown[] }).matches,
		}),
	},
	terminal_get_row_text: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/row-text?row=${args.row}`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_get_lines: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/lines?start=${args.start}&end=${args.end}`,
			transform: (data) => (data as { lines: string[] }).lines,
		}),
	},
	terminal_styled_rows: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/styled-rows?start=${args.start}&count=${args.count}`,
		}),
	},
	terminal_get_cursor_line: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/cursor-line`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_hyperlink_at: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/hyperlink?row=${args.row}&col=${args.col}`,
			transform: (data) => (data as { url: string | null }).url,
		}),
	},
	terminal_hyperlink_span: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/hyperlink-span?row=${args.row}&col=${args.col}`,
			// Option<(start,end,url)> -> [start,end,url] | null; pass null through the empty-body guard.
			transform: (data) => data ?? null,
		}),
	},
	terminal_get_selection_text: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/selection-text?startRow=${args.startRow}&startCol=${args.startCol}&endRow=${args.endRow}&endCol=${args.endCol}`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_get_logical_line: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/logical-line?row=${args.row}`,
		}),
	},
	terminal_request_frame: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/request-frame`,
		}),
	},

	// --- Orchestrator ---
	get_orchestrator_stats: { map: () => ({ method: "GET", path: "/stats" }) },
	get_session_metrics: { map: () => ({ method: "GET", path: "/metrics" }) },
	get_process_stats: { map: () => ({ method: "GET", path: "/process/stats" }) },

	// --- Claude Usage dashboard ---
	get_claude_usage_api: { map: () => ({ method: "GET", path: "/claude/usage" }) },
	get_claude_project_list: { map: () => ({ method: "GET", path: "/claude/projects" }) },
	get_claude_usage_timeline: {
		map: (args, p) => {
			let path = `/claude/timeline?scope=${p("scope")}`;
			if (args.days != null) path += `&days=${encodeURIComponent(String(args.days))}`;
			return { method: "GET", path };
		},
	},
	get_claude_session_stats: {
		map: (_args, p) => ({ method: "GET", path: `/claude/session-stats?scope=${p("scope")}` }),
	},
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

	// --- Config: pane layout ---
	load_pane_layout: { map: () => ({ method: "GET", path: "/config/pane-layout" }) },
	save_pane_layout: {
		map: (args) => ({ method: "PUT", path: "/config/pane-layout", body: args.layout }),
	},

	// --- Config: caches ---
	clear_caches: { map: () => ({ method: "POST", path: "/config/clear-caches" }) },
	clear_repo_caches: { map: (a) => ({ method: "POST", path: `/config/clear-repo-caches`, body: { path: a.path } }) },

	// --- Config: repo local config (.tuic.json) ---
	load_repo_local_config: {
		map: (_args, p) => ({ method: "GET", path: `/config/repo-local-config?path=${p("repoPath")}` }),
	},

	// --- Config: prompt library ---
	load_prompt_library: { map: () => ({ method: "GET", path: "/config/prompt-library" }) },
	save_prompt_library: {
		map: (args) => ({ method: "PUT", path: "/config/prompt-library", body: args.config }),
	},

	// --- Config: activity ---
	load_activity: { map: () => ({ method: "GET", path: "/config/activity" }) },
	save_activity: {
		map: (args) => ({ method: "PUT", path: "/config/activity", body: args.items ?? args }),
	},

	// --- Config: keybindings ---
	load_keybindings: { map: () => ({ method: "GET", path: "/config/keybindings" }) },
	save_keybindings: {
		map: (args) => ({ method: "PUT", path: "/config/keybindings", body: args.config }),
	},

	// --- Config: agents ---
	load_agents_config: { map: () => ({ method: "GET", path: "/config/agents" }) },
	save_agents_config: {
		map: (args) => ({ method: "PUT", path: "/config/agents", body: args.config }),
	},
	// Hook instrumentation toggle: GET returns {state}, the Tauri command returns the
	// bare AgentHookState string — unwrap it. PUT's {ok:true} is discarded by callers.
	get_agent_hook_state: {
		map: (_args, p) => ({
			method: "GET",
			path: `/config/agents/${p("agentType")}/hook-instrumentation`,
			transform: (data) => (data as { state: string }).state,
		}),
	},
	set_agent_hook_instrumentation: {
		map: (args, p) => ({
			method: "PUT",
			path: `/config/agents/${p("agentType")}/hook-instrumentation`,
			body: { enabled: args.enabled },
		}),
	},

	// --- Plugin data (read-only here; write/delete routes are tracked in story 071) ---
	// Tauri contract is Option<String>: missing key → null. The route 404s on miss,
	// which notFoundAsNull bridges back to null. Found content is returned as a string
	// to match the command's String payload (the route may sniff JSON and parse it).
	read_plugin_data: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/data/${p("path")}`,
			notFoundAsNull: true,
			transform: (data) => (data == null ? null : typeof data === "string" ? data : JSON.stringify(data)),
		}),
	},

	// --- Config: provider registry ---
	load_provider_registry: { map: () => ({ method: "GET", path: "/config/provider-registry" }) },
	save_provider_registry: {
		map: (args) => ({ method: "PUT", path: "/config/provider-registry", body: args.registry }),
	},

	// --- Git/GitHub ---
	get_repo_info: {
		map: (_args, p) => ({ method: "GET", path: `/repo/info?path=${p("path")}` }),
	},

	// --- Git panel (story 064) ---
	get_gutter_changes: {
		map: (args, p) => {
			let path = `/repo/gutter-changes?path=${p("path")}&file=${p("file")}`;
			if (args.scope != null) path += `&scope=${encodeURIComponent(String(args.scope))}`;
			return { method: "GET", path };
		},
	},
	get_branches_detail: {
		map: (_args, p) => ({ method: "GET", path: `/repo/branches-detail?path=${p("path")}` }),
	},
	get_recent_branches: {
		map: (args, p) => {
			let path = `/repo/recent-branches?path=${p("path")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	get_branch_base: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/branch-base?path=${p("path")}&branchName=${p("branchName")}`,
			// Option<String> -> null on miss; pass null through the empty-body guard.
			transform: (data) => data ?? null,
		}),
	},
	check_worktree_dirty: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/worktree-dirty?repoPath=${p("repoPath")}&branchName=${p("branchName")}`,
		}),
	},
	list_base_ref_options: {
		map: (_args, p) => ({ method: "GET", path: `/repo/base-ref-options?repoPath=${p("repoPath")}` }),
	},
	generate_clone_branch_name_cmd: {
		map: (args) => ({
			method: "POST",
			path: "/repo/clone-branch-name",
			body: { sourceBranch: args.sourceBranch, existingNames: args.existingNames },
		}),
	},
	get_commit_graph: {
		map: (args, p) => {
			let path = `/repo/commit-graph?path=${p("path")}`;
			if (args.count != null) path += `&count=${encodeURIComponent(String(args.count))}`;
			return { method: "GET", path };
		},
	},
	create_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-branch",
			body: { path: args.path, name: args.name, startPoint: args.startPoint, checkout: args.checkout },
		}),
	},
	delete_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/delete-branch",
			body: { path: args.path, name: args.name, force: args.force },
		}),
	},
	delete_local_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/delete-local-branch",
			body: { repoPath: args.repoPath, branchName: args.branchName, keepWorktree: args.keepWorktree },
		}),
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
	github_start_polling: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/start",
			body: { paths: args.paths, issueFilter: args.issueFilter },
		}),
	},
	github_stop_polling: {
		map: () => ({ method: "POST", path: "/repo/github-poller/stop" }),
	},
	github_set_visibility: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/visibility",
			body: { visible: args.visible },
		}),
	},
	github_poll_repo: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/poll-repo",
			body: { path: args.path },
		}),
	},
	github_update_paths: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/update-paths",
			body: { paths: args.paths },
		}),
	},
	github_set_issue_filter: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/set-issue-filter",
			body: { filter: args.filter },
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
	git_apply_reverse_patch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/apply-reverse-patch",
			body: { path: args.path, patch: args.patch, scope: args.scope },
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
		map: (args, p) => {
			const force = args.force === true ? "&force=true" : "";
			return {
				method: "DELETE",
				path: `/worktrees/${p("branchName")}?repoPath=${p("repoPath")}&deleteBranch=${args.deleteBranch ?? true}${force}`,
			};
		},
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

	resolve_context_variables: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/resolve-variables",
			body: { repoPath: args.repoPath },
		}),
	},
	resolve_prompt_variables: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/resolve-prompt-variables",
			body: { content: args.content, repoPath: args.repoPath },
		}),
	},
	execute_headless_prompt: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/execute-headless",
			body: {
				command: args.command,
				args: args.args,
				stdinContent: args.stdinContent,
				timeoutMs: args.timeoutMs,
				repoPath: args.repoPath,
				env: args.env,
			},
		}),
	},
	execute_api_prompt: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/execute-api",
			body: {
				systemPrompt: args.systemPrompt,
				content: args.content,
				timeoutMs: args.timeoutMs,
			},
		}),
	},

	// --- Agents ---
	verify_agent_session: {
		map: (args) => ({
			method: "POST",
			path: "/agents/verify-session",
			body: { agentType: args.agentType, sessionId: args.sessionId, cwd: args.cwd },
		}),
	},
	detect_agents: { map: () => ({ method: "GET", path: "/agents" }) },
	detect_all_agent_binaries: {
		map: (args) => ({ method: "POST", path: "/agents/detect-all", body: { binaries: args.binaries } }),
	},
	detect_agent_binary: {
		map: (_args, p) => ({ method: "GET", path: `/agents/detect?binary=${p("binary")}` }),
	},
	detect_installed_ides: { map: () => ({ method: "GET", path: "/agents/ides" }) },

	// --- Watchers ---
	start_repo_watcher: {
		map: (_args, p) => ({ method: "POST", path: `/watchers/repo?path=${p("repoPath")}` }),
	},
	stop_repo_watcher: {
		map: (_args, p) => ({ method: "DELETE", path: `/watchers/repo?path=${p("repoPath")}` }),
	},
	set_hot_repos: {
		map: (args) => ({ method: "PUT", path: "/watchers/hot-repos", body: args }),
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
	search_files: {
		map: (args, p) => {
			let path = `/fs/search?repoPath=${p("repoPath")}&query=${p("query")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	fs_read_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_editor_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_external_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-external?path=${p("path")}` }),
	},
	read_editor_file_external: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor-external?path=${p("path")}` }),
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
	// Returns Option<ResolvedFilePath>: a miss serializes to JSON null, so the
	// transform passes null straight through (no empty-body error).
	resolve_terminal_path: {
		map: (_args, p) => ({
			method: "GET",
			path: `/fs/resolve-terminal-path?cwd=${p("cwd")}&candidate=${p("candidate")}`,
			transform: (data) => data ?? null,
		}),
	},
	stat_path: {
		map: (_args, p) => ({ method: "GET", path: `/fs/stat?path=${p("path")}` }),
	},
	warm_content_index: {
		map: (args) => ({ method: "POST", path: "/fs/warm-index", body: { repoPath: args.repoPath } }),
	},
	write_external_file: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write-external",
			body: { path: args.path, content: args.content },
		}),
	},
	copy_path_abs: {
		map: (args) => ({ method: "POST", path: "/fs/copy-abs", body: { from: args.from, to: args.to } }),
	},
	move_path_abs: {
		map: (args) => ({ method: "POST", path: "/fs/move-abs", body: { from: args.from, to: args.to } }),
	},
	fs_transfer_paths: {
		map: (args) => ({
			method: "POST",
			path: "/fs/transfer",
			body: {
				destDir: args.destDir,
				paths: args.paths,
				mode: args.mode,
				allowRecursive: args.allowRecursive,
			},
		}),
	},
	search_content: {
		map: (args, p) => {
			let path = `/fs/search-content?repoPath=${p("repoPath")}&query=${p("query")}&caseSensitive=${p("caseSensitive")}&useRegex=${p("useRegex")}&wholeWord=${p("wholeWord")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	search_content_all: {
		map: (args, p) => {
			let path = `/fs/search-content-all?query=${p("query")}&caseSensitive=${p("caseSensitive")}`;
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

	// --- Remote Connections ---
	list_remote_connections: { map: () => ({ method: "GET", path: "/config/remote-connections" }) },
	save_remote_connection: {
		map: (args) => ({ method: "PUT", path: "/config/remote-connections", body: args.connection }),
	},
	delete_remote_connection: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}` }),
	},

	// --- Tunnels ---
	list_tunnel_profiles: { map: () => ({ method: "GET", path: "/tunnels/profiles" }) },
	save_tunnel_profile: { map: (args) => ({ method: "POST", path: "/tunnels/profiles", body: args.profile }) },
	delete_tunnel_profile: { map: (args) => ({ method: "DELETE", path: `/tunnels/profiles/${args.id}` }) },
	start_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/start/${args.id}` }) },
	stop_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/stop/${args.id}` }) },
	list_active_tunnels: { map: () => ({ method: "GET", path: "/tunnels/active" }) },
	get_tunnel_status: { map: (args) => ({ method: "GET", path: `/tunnels/status/${args.id}` }) },
	get_tunnel_audit: { map: (args) => ({ method: "GET", path: `/tunnels/audit/${args.id}?limit=${args.limit || 20}` }) },
	list_ssh_config_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts" }) },
	list_ssh_agent_keys: { map: () => ({ method: "GET", path: "/tunnels/agent-keys" }) },

	// --- App Logger ---
	push_log: {
		map: (args) => ({
			method: "POST",
			path: "/logs",
			body: {
				level: args.level,
				source: args.source,
				message: args.message,
				data_json: args.dataJson,
				audience: args.audience,
			},
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
	const p: ArgEncoder = (key) => encodeArg(command, args, key);
	return entry.map(args, p);
}

/** Build a full URL for HTTP transport using current window origin or a remote baseUrl */
export function buildHttpUrl(path: string, baseUrl?: string): string {
	if (baseUrl) return `${baseUrl}${path}`;
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
 * Per-session write queue — serializes write_pty calls in browser mode to prevent
 * letter reordering when typing fast. Parallel HTTP POSTs can arrive out of order;
 * chaining them ensures each write completes before the next is sent.
 */
const _writeQueues = new Map<string, Promise<unknown>>();

function queuedWrite<T>(sessionId: string, fn: () => Promise<T>): Promise<T> {
	const prev = _writeQueues.get(sessionId) ?? Promise.resolve();
	const next = prev.then(fn, fn); // Always chain, even on prior failure
	_writeQueues.set(sessionId, next);
	// Clean up when queue drains
	next.then(
		() => {
			if (_writeQueues.get(sessionId) === next) _writeQueues.delete(sessionId);
		},
		() => {
			if (_writeQueues.get(sessionId) === next) _writeQueues.delete(sessionId);
		},
	);
	return next;
}

/**
 * RPC call — uses Tauri invoke() or HTTP fetch() based on environment.
 * Concurrent identical idempotent calls are coalesced into a single in-flight request.
 * write_pty calls are serialized per-session in browser mode to prevent reordering.
 * Usage: `const result = await rpc<string>("create_pty", { config });`
 */
export function rpc<T>(command: string, args: Record<string, unknown> = {}, connectionId?: string): Promise<T> {
	// Serialize write_pty per session in browser mode to prevent letter reordering
	if (command === "write_pty" && (!isTauri() || connectionId)) {
		const sessionId = (args.sessionId ?? args.id) as string;
		if (sessionId) {
			return queuedWrite(sessionId, () => rpcImpl<T>(command, args, connectionId));
		}
	}
	if (isIdempotentRpc(command, args)) {
		const key = connectionId
			? `${connectionId}:${command}:${JSON.stringify(args)}`
			: `${command}:${JSON.stringify(args)}`;
		const existing = _inflight.get(key) as Promise<T> | undefined;
		if (existing) return existing;
		const promise = rpcImpl<T>(command, args, connectionId).finally(() => _inflight.delete(key));
		_inflight.set(key, promise as Promise<unknown>);
		return promise;
	}
	return rpcImpl<T>(command, args, connectionId);
}

async function rpcImpl<T>(command: string, args: Record<string, unknown>, connectionId?: string): Promise<T> {
	// When connectionId is provided, always use HTTP fetch (remote daemon is accessed via HTTP)
	if (!connectionId && isTauri()) {
		const { invoke } = await import("@tauri-apps/api/core");
		// Only pass args if non-empty (matches Tauri invoke signature)
		if (Object.keys(args).length > 0) {
			return invoke<T>(command, args);
		}
		return invoke<T>(command);
	}

	const mapping = mapCommandToHttp(command, args);
	const baseUrl = connectionId ? remoteConnectionsStore.getBaseUrl(connectionId) : undefined;
	if (connectionId && !baseUrl) {
		throw new Error(`Remote connection ${connectionId} not connected`);
	}
	const url = buildHttpUrl(mapping.path, baseUrl);

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
		if (resp.status === 404 && mapping.notFoundAsNull) {
			return null as T;
		}
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
	/** Called when WebSocket drops and reconnect is attempted (browser mode only). */
	onReconnecting?: (attempt: number, maxAttempts: number) => void;
	/** Called when WebSocket reconnect succeeds (browser mode only). */
	onReconnected?: () => void;
}

export async function subscribePty(
	sessionId: string,
	onData: (data: string) => void,
	onExit: () => void,
	onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions,
): Promise<Unsubscribe> {
	// Normalize overloaded 4th param: function (legacy) or options object
	const opts: SubscribePtyOptions =
		typeof onParsedOrOptions === "function" ? { onParsed: onParsedOrOptions } : (onParsedOrOptions ?? {});
	const onParsed = opts.onParsed;
	if (isTauri()) {
		const { listen } = await import("@tauri-apps/api/event");
		const unlistenOutput = await listen<{ data: string }>(`pty-output-${sessionId}`, (event) => {
			onData(event.payload.data);
		});
		const unlistenExit = await listen(`pty-exit-${sessionId}`, () => {
			onExit();
		});
		// Idempotent dispose: Tauri's internal listener registry crashes
		// on double-unregister (listeners[eventId].handlerId on undefined).
		let disposed = false;
		return () => {
			if (disposed) return;
			disposed = true;
			try {
				unlistenOutput();
			} catch (err) {
				// Swallow: listener already gone (e.g. session exit race)
				void err;
			}
			try {
				unlistenExit();
			} catch (err) {
				void err;
			}
		};
	}

	// Browser mode: WebSocket with JSON framing and auto-reconnect
	const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
	const queryFormat = opts.format === "log" ? "log" : opts.stripAnsi ? "text" : null;

	// Track server-side write offset for delta catch-up on reconnect
	let lastTotalWritten: number | null = null;
	let disposed = false;
	let activeWs: WebSocket | null = null;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

	const handleMessage = (event: MessageEvent) => {
		const raw = event.data as string;
		// JSON frame detection: starts with { and contains "type"
		if (raw.startsWith("{")) {
			try {
				const frame = JSON.parse(raw) as WsParsedEvent;
				// Track total_written for reconnect delta
				if (typeof (frame as Record<string, unknown>).total_written === "number") {
					lastTotalWritten = (frame as Record<string, unknown>).total_written as number;
				}
				switch (frame.type) {
					case "output":
						onData(frame.data as string);
						break;
					case "log": {
						// Track the monotonic line cursor so reconnect resumes from the last
						// line we consumed instead of replaying from the mount offset (which
						// duplicated the whole scrollback on every WS reconnect).
						const logCursor = (frame as Record<string, unknown>).total_lines;
						if (typeof logCursor === "number") {
							lastTotalWritten = logCursor;
						}
						const lines = frame.lines as LogLine[] | undefined;
						if (lines && lines.length > 0) {
							if (opts.onLogLines) {
								opts.onLogLines(lines);
							} else {
								// Backward compat: join span texts as plain string
								const texts = lines.map((l) => {
									if (typeof l === "string") return l;
									if (l && typeof l === "object" && "spans" in l) {
										return ((l as { spans: { text: string }[] }).spans || []).map((s) => s.text).join("");
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
						disposed = true; // Session truly ended — don't reconnect
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

	/** Build the WS URL with current params (including offset for reconnect). */
	const buildWsUrl = (reconnectOffset?: number | null): string => {
		const params = new URLSearchParams();
		if (queryFormat) params.set("format", queryFormat);
		if (reconnectOffset != null) {
			params.set("offset", String(reconnectOffset));
		} else if (opts.logOffset != null) {
			params.set("offset", String(opts.logOffset));
		}
		const query = params.size > 0 ? `?${params}` : "";
		return `${protocol}//${window.location.host}/sessions/${sessionId}/stream${query}`;
	};

	/** Connect (or reconnect) the WebSocket. */
	const connect = (reconnectOffset?: number | null): Promise<void> =>
		new Promise<void>((resolve, reject) => {
			const wsUrl = buildWsUrl(reconnectOffset);
			const ws = new WebSocket(wsUrl);
			activeWs = ws;

			ws.onopen = () => {
				appLogger.debug("network", `WebSocket connected: ${sessionId}`);
				// Re-wire onclose for live session
				ws.onclose = (evt: CloseEvent) => {
					if (disposed) return;
					if (evt.code === 1000 || evt.code === 1001) {
						// Normal close or going away — don't reconnect
						onExit();
						return;
					}
					appLogger.debug("network", `WebSocket closed abnormally (code ${evt.code}), will reconnect`);
					scheduleReconnect();
				};
				resolve();
			};

			ws.onerror = () => {
				// onerror is always followed by onclose, so reject is handled there
			};

			ws.onclose = (evt: CloseEvent) => {
				reject(new Error(`WebSocket closed before opening (code ${evt.code}): ${evt.reason || "no reason"}`));
			};

			ws.onmessage = handleMessage;
		});

	// Reconnect with exponential backoff
	const MAX_RETRIES = 10;
	const BASE_DELAY_MS = 1000;
	const MAX_DELAY_MS = 30_000;
	let retryCount = 0;

	const scheduleReconnect = () => {
		if (disposed) return;
		if (retryCount >= MAX_RETRIES) {
			appLogger.warn("network", `WebSocket reconnect failed after ${MAX_RETRIES} attempts: ${sessionId}`);
			onExit();
			return;
		}
		const delay = Math.min(BASE_DELAY_MS * 2 ** retryCount, MAX_DELAY_MS);
		retryCount++;
		appLogger.debug("network", `WebSocket reconnecting in ${delay}ms (attempt ${retryCount}/${MAX_RETRIES})`);
		opts.onReconnecting?.(retryCount, MAX_RETRIES);
		reconnectTimer = setTimeout(async () => {
			if (disposed) return;
			try {
				await connect(lastTotalWritten);
				retryCount = 0; // Reset on success
				opts.onReconnected?.();
			} catch {
				// connect() failed (e.g. session gone → 404 triggers immediate close)
				scheduleReconnect();
			}
		}, delay);
	};

	// Initial connection
	await connect();

	return () => {
		disposed = true;
		if (reconnectTimer) clearTimeout(reconnectTimer);
		activeWs?.close();
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
	baseUrl?: string,
): Promise<Unsubscribe> {
	if (!baseUrl && isTauri()) {
		const { listen } = await import("@tauri-apps/api/event");
		const unsubscribers: Array<() => void> = [];
		for (const [eventType, handler] of Object.entries(handlers)) {
			const unlisten = await listen(eventType, (event) => handler(event.payload));
			unsubscribers.push(unlisten);
		}
		return () => unsubscribers.forEach((fn) => fn());
	}

	// Browser/remote mode: SSE via EventSource
	const types = Object.keys(handlers).join(",");
	const url = buildHttpUrl(`/events?types=${encodeURIComponent(types)}`, baseUrl);
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
