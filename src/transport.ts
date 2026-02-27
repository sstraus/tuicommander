/**
 * Transport abstraction layer — auto-detects Tauri IPC vs HTTP/WebSocket.
 *
 * In Tauri mode: uses invoke() for RPC, listen() for events.
 * In browser mode: uses fetch() for RPC, WebSocket for PTY streaming.
 */

import { appLogger } from "./stores/appLogger";

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

/** Commands that require local hardware/OS integration and cannot work in browser mode */
const BROWSER_UNSUPPORTED_COMMANDS = new Set([
  // Dictation (requires local audio hardware)
  "get_dictation_status",
  "get_model_info",
  "download_whisper_model",
  "delete_whisper_model",
  "start_dictation",
  "stop_dictation_and_transcribe",
  "get_correction_map",
  "set_correction_map",
  "list_audio_devices",
  "inject_text",
  "get_dictation_config",
  "set_dictation_config",
  // OS integration
  "open_in_app",
]);

/** Map a Tauri invoke command + args to an HTTP method/path/body */
export function mapCommandToHttp(command: string, args: Record<string, unknown>): HttpMapping {
  if (BROWSER_UNSUPPORTED_COMMANDS.has(command)) {
    throw new Error(
      `Command "${command}" requires native OS features and is not available in browser mode`,
    );
  }

  const p = (key: string): string => {
    const val = args[key];
    if (val === undefined || val === null) {
      throw new Error(`mapCommandToHttp(${command}): missing required argument "${key}"`);
    }
    return encodeURIComponent(String(val));
  };

  switch (command) {
    // --- Session lifecycle ---
    case "create_pty": {
      const config = args.config as Record<string, unknown>;
      return {
        method: "POST",
        path: "/sessions",
        body: config,
        transform: (data: unknown) => (data as { session_id: string }).session_id,
      };
    }
    case "create_pty_with_worktree":
      return {
        method: "POST",
        path: "/sessions/worktree",
        body: {
          config: args.config,
          base_repo: args.baseRepo,
          branch_name: args.branchName,
        },
      };
    case "write_pty":
      return {
        method: "POST",
        path: `/sessions/${args.sessionId ?? args.id}/write`,
        body: { data: args.data },
      };
    case "resize_pty":
      return {
        method: "POST",
        path: `/sessions/${args.sessionId}/resize`,
        body: { rows: args.rows, cols: args.cols },
      };
    case "pause_pty":
      return { method: "POST", path: `/sessions/${args.sessionId}/pause` };
    case "resume_pty":
      return { method: "POST", path: `/sessions/${args.sessionId}/resume` };
    case "get_kitty_flags":
      return { method: "GET", path: `/sessions/${args.sessionId}/kitty-flags` };
    case "close_pty":
      return { method: "DELETE", path: `/sessions/${args.sessionId}` };
    case "get_session_foreground_process":
      return {
        method: "GET",
        path: `/sessions/${args.sessionId}/foreground`,
        transform: (data) => (data as { agent: string | null }).agent,
      };

    // --- Orchestrator ---
    case "get_orchestrator_stats":
      return { method: "GET", path: "/stats" };
    case "get_session_metrics":
      return { method: "GET", path: "/metrics" };
    case "list_active_sessions":
      return { method: "GET", path: "/sessions" };
    case "can_spawn_session":
      return {
        method: "GET",
        path: "/stats",
        transform: (data) => {
          const stats = data as { active_sessions: number; max_sessions: number };
          return stats.active_sessions < stats.max_sessions;
        },
      };

    // --- Config: app ---
    case "load_config":
      return { method: "GET", path: "/config" };
    case "save_config":
      return { method: "PUT", path: "/config", body: args.config };
    case "hash_password":
      return {
        method: "POST",
        path: "/config/hash-password",
        body: { password: args.password },
        transform: (data) => (data as { hash: string }).hash,
      };

    // --- Config: notifications ---
    case "load_notification_config":
      return { method: "GET", path: "/config/notifications" };
    case "save_notification_config":
      return { method: "PUT", path: "/config/notifications", body: args.config };

    // --- Config: UI prefs ---
    case "load_ui_prefs":
      return { method: "GET", path: "/config/ui-prefs" };
    case "save_ui_prefs":
      return { method: "PUT", path: "/config/ui-prefs", body: args.config };

    // --- Config: repo settings ---
    case "load_repo_settings":
      return { method: "GET", path: "/config/repo-settings" };
    case "save_repo_settings":
      return { method: "PUT", path: "/config/repo-settings", body: args.config };
    case "check_has_custom_settings":
      return { method: "GET", path: `/config/repo-settings/has-custom?path=${p("path")}` };
    case "load_repo_defaults":
      return { method: "GET", path: "/config/repo-defaults" };
    case "save_repo_defaults":
      return { method: "PUT", path: "/config/repo-defaults", body: args.config };

    // --- Config: repositories ---
    case "load_repositories":
      return { method: "GET", path: "/config/repositories" };
    case "save_repositories":
      return { method: "PUT", path: "/config/repositories", body: args.config };

    // --- Config: prompt library ---
    case "load_prompt_library":
      return { method: "GET", path: "/config/prompt-library" };
    case "save_prompt_library":
      return { method: "PUT", path: "/config/prompt-library", body: args.config };

    // --- Git/GitHub ---
    case "get_repo_info":
      return { method: "GET", path: `/repo/info?path=${p("path")}` };
    case "get_git_diff":
      return {
        method: "GET",
        path: `/repo/diff?path=${p("path")}`,
        transform: (data) => (data as { diff: string }).diff,
      };
    case "get_diff_stats":
      return { method: "GET", path: `/repo/diff-stats?path=${p("path")}` };
    case "get_changed_files":
      return { method: "GET", path: `/repo/files?path=${p("path")}` };
    case "get_file_diff":
      return {
        method: "GET",
        path: `/repo/file-diff?path=${p("path")}&file=${p("file")}`,
      };
    case "get_github_status":
      return { method: "GET", path: `/repo/github?path=${p("path")}` };
    case "get_repo_pr_statuses":
      return { method: "GET", path: `/repo/prs?path=${p("path")}` };
    case "get_all_pr_statuses":
      return {
        method: "POST",
        path: "/repo/prs/batch",
        body: { paths: args.paths, include_merged: args.includeMerged },
      };
    case "get_git_branches":
      return { method: "GET", path: `/repo/branches?path=${p("path")}` };
    case "get_merged_branches":
      return { method: "GET", path: `/repo/branches/merged?path=${p("repoPath")}` };
    case "get_ci_checks":
      return { method: "GET", path: `/repo/ci?path=${p("path")}&pr_number=${p("prNumber")}` };
    case "rename_branch":
      return {
        method: "POST",
        path: "/repo/branch/rename",
        body: { path: args.path, old_name: args.oldName, new_name: args.newName },
      };
    case "get_initials":
      return { method: "GET", path: `/repo/initials?name=${p("name")}` };
    case "check_is_main_branch":
      return {
        method: "GET",
        path: `/repo/is-main-branch?branch=${p("branch")}`,
      };

    // --- Worktrees ---
    case "list_worktrees":
      return { method: "GET", path: "/worktrees" };
    case "get_worktrees_dir": {
      const rp = args?.repoPath as string | undefined;
      return {
        method: "GET",
        path: rp ? `/worktrees/dir?repo_path=${encodeURIComponent(rp)}` : "/worktrees/dir",
        transform: (data) => (data as { dir: string }).dir,
      };
    }
    case "get_worktree_paths":
      return { method: "GET", path: `/worktrees/paths?path=${p("repoPath")}` };
    case "create_worktree":
      return {
        method: "POST",
        path: "/worktrees",
        body: { base_repo: args.baseRepo, branch_name: args.branchName, base_ref: args.baseRef },
      };
    case "remove_worktree":
      return {
        method: "DELETE",
        path: `/worktrees/${p("branchName")}?repoPath=${p("repoPath")}&deleteBranch=${args.deleteBranch ?? true}`,
      };
    case "generate_worktree_name_cmd":
      return {
        method: "POST",
        path: "/worktrees/generate-name",
        body: { existing_names: args.existingNames },
      };
    case "finalize_merged_worktree":
      return {
        method: "POST",
        path: "/worktrees/finalize",
        body: { repoPath: args.repoPath, branchName: args.branchName, action: args.action },
      };

    case "list_local_branches":
      return { method: "GET", path: `/repo/local-branches?path=${p("repoPath")}` };

    // --- File operations ---
    case "list_markdown_files":
      return { method: "GET", path: `/repo/markdown-files?path=${p("path")}` };
    case "read_file":
      return { method: "GET", path: `/repo/file?path=${p("path")}&file=${p("file")}` };

    // --- Prompt processing ---
    case "process_prompt_content":
      return {
        method: "POST",
        path: "/prompt/process",
        body: { content: args.content, variables: args.variables },
      };
    case "extract_prompt_variables":
      return {
        method: "POST",
        path: "/prompt/extract-variables",
        body: { content: args.content },
      };

    // --- Agents ---
    case "detect_agents":
      return { method: "GET", path: "/agents" };
    case "detect_agent_binary":
      return { method: "GET", path: `/agents/detect?binary=${p("binary")}` };
    case "detect_installed_ides":
      return { method: "GET", path: "/agents/ides" };

    // --- MCP status ---
    case "get_mcp_status":
      return { method: "GET", path: "/mcp/status" };

    // --- Network ---
    case "get_local_ip":
      return { method: "GET", path: "/system/local-ip" };
    case "get_local_ips":
      return { method: "GET", path: "/system/local-ips" };

    // --- File browser ---
    case "list_directory":
      return { method: "GET", path: `/fs/list?repoPath=${p("repoPath")}&subdir=${p("subdir")}` };
    case "fs_read_file":
      return { method: "GET", path: `/fs/read?repoPath=${p("repoPath")}&file=${p("file")}` };
    case "write_file":
      return {
        method: "POST",
        path: "/fs/write",
        body: { repoPath: args.repoPath, file: args.file, content: args.content },
      };
    case "create_directory":
      return {
        method: "POST",
        path: "/fs/mkdir",
        body: { repoPath: args.repoPath, dir: args.dir },
      };
    case "delete_path":
      return {
        method: "POST",
        path: "/fs/delete",
        body: { repoPath: args.repoPath, path: args.path },
      };
    case "rename_path":
      return {
        method: "POST",
        path: "/fs/rename",
        body: { repoPath: args.repoPath, from: args.from, to: args.to },
      };
    case "copy_path":
      return {
        method: "POST",
        path: "/fs/copy",
        body: { repoPath: args.repoPath, from: args.from, to: args.to },
      };
    case "add_to_gitignore":
      return {
        method: "POST",
        path: "/fs/gitignore",
        body: { repoPath: args.repoPath, pattern: args.pattern },
      };

    // --- Notes ---
    case "load_notes":
      return { method: "GET", path: "/config/notes" };
    case "save_notes":
      return { method: "PUT", path: "/config/notes", body: args.config };

    // --- Recent commits ---
    case "get_recent_commits":
      return { method: "GET", path: `/repo/recent-commits?path=${p("path")}&count=${args.count ?? 5}` };

    // --- Plugins ---
    case "list_user_plugins":
      return { method: "GET", path: "/plugins/list" };

    // --- App Logger ---
    case "push_log":
      return {
        method: "POST",
        path: "/logs",
        body: { level: args.level, source: args.source, message: args.message, data_json: args.dataJson },
      };
    case "get_logs":
      return { method: "GET", path: `/logs?limit=${args.limit ?? 0}` };
    case "clear_logs":
      return { method: "DELETE", path: "/logs" };

    default:
      throw new Error(`No HTTP mapping for command: ${command}`);
  }
}

/** Build a full URL for HTTP transport using current window origin */
export function buildHttpUrl(path: string): string {
  if (typeof window !== "undefined" && window.location?.origin) {
    return `${window.location.origin}${path}`;
  }
  return path;
}

/**
 * RPC call — uses Tauri invoke() or HTTP fetch() based on environment.
 * Usage: `const result = await rpc<string>("create_pty", { config });`
 */
export async function rpc<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
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

/**
 * Subscribe to PTY session events.
 *
 * In Tauri: uses listen() for pty-output-{sessionId}, pty-exit-{sessionId}.
 * In browser: uses WebSocket to /sessions/{sessionId}/stream.
 *
 * @param sessionId - PTY session ID
 * @param onData - Called with each chunk of PTY output
 * @param onExit - Called when the session exits
 * @returns Promise resolving to an unsubscribe function
 */
export async function subscribePty(
  sessionId: string,
  onData: (data: string) => void,
  onExit: () => void,
): Promise<Unsubscribe> {
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

  // Browser mode: WebSocket
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const wsUrl = `${protocol}//${window.location.host}/sessions/${sessionId}/stream`;
  const ws = new WebSocket(wsUrl);

  // Wait for connection to open. Wire all handlers inside the promise so
  // a pre-handshake close (e.g. session not found) correctly rejects.
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = (e) => reject(new Error(`WebSocket connection failed: ${e}`));
    ws.onclose = (event: CloseEvent) => {
      reject(new Error(`WebSocket closed before opening (code ${event.code}): ${event.reason || "no reason"}`));
    };
    ws.onmessage = (event) => onData(event.data);
  });

  // Re-wire onclose for the live session after successful open
  ws.onclose = (event: CloseEvent) => {
    if (!event.wasClean) {
      appLogger.warn("network", `WebSocket closed abnormally (code ${event.code}): ${event.reason || "unknown"}`);
    }
    onExit();
  };

  return () => {
    ws.close();
  };
}
