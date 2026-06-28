import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { INTENTIONALLY_UNMAPPED, buildHttpUrl, isTauri, mapCommandToHttp } from "../transport";

describe("transport", () => {
	describe("isTauri()", () => {
		const original = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		afterEach(() => {
			if (original !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = original;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("returns true when __TAURI_INTERNALS__ exists", () => {
			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
			expect(isTauri()).toBe(true);
		});

		it("returns false when __TAURI_INTERNALS__ is absent", () => {
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			expect(isTauri()).toBe(false);
		});
	});

	describe("buildHttpUrl()", () => {
		it("builds URL with current origin by default", () => {
			const url = buildHttpUrl("/health");
			// In test env, location.origin may be empty string, so just check it ends with /health
			expect(url).toContain("/health");
		});
	});

	describe("mapCommandToHttp()", () => {
		it("maps create_pty to POST /sessions", () => {
			const result = mapCommandToHttp("create_pty", { config: { rows: 24, cols: 80, shell: null, cwd: "/tmp" } });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions");
			expect(result.body).toEqual({ rows: 24, cols: 80, shell: null, cwd: "/tmp" });
		});

		it("maps write_pty to POST /sessions/{id}/write", () => {
			const result = mapCommandToHttp("write_pty", { sessionId: "abc", data: "hello" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/write");
			expect(result.body).toEqual({ data: "hello" });
		});

		it("maps resize_pty to POST /sessions/{id}/resize", () => {
			const result = mapCommandToHttp("resize_pty", { sessionId: "abc", rows: 40, cols: 120 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/resize");
			expect(result.body).toEqual({ rows: 40, cols: 120 });
		});

		it("maps pause_pty to POST /sessions/{id}/pause", () => {
			const result = mapCommandToHttp("pause_pty", { sessionId: "abc" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/pause");
		});

		it("maps resume_pty to POST /sessions/{id}/resume", () => {
			const result = mapCommandToHttp("resume_pty", { sessionId: "abc" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/abc/resume");
		});

		it("maps close_pty to DELETE /sessions/{id}", () => {
			const result = mapCommandToHttp("close_pty", { sessionId: "abc", cleanupWorktree: false });
			expect(result.method).toBe("DELETE");
			expect(result.path).toBe("/sessions/abc");
		});

		it("maps get_session_foreground_process to GET /sessions/{id}/foreground", () => {
			const result = mapCommandToHttp("get_session_foreground_process", { sessionId: "abc" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/abc/foreground");
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ agent: "claude" })).toBe("claude");
			expect(result.transform?.({ agent: null })).toBeNull();
		});

		it("maps get_orchestrator_stats to GET /stats", () => {
			const result = mapCommandToHttp("get_orchestrator_stats", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/stats");
		});

		it("maps get_session_metrics to GET /metrics", () => {
			const result = mapCommandToHttp("get_session_metrics", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/metrics");
		});

		it("maps list_active_sessions to GET /sessions", () => {
			const result = mapCommandToHttp("list_active_sessions", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions");
		});

		it("maps can_spawn_session to GET /stats", () => {
			const result = mapCommandToHttp("can_spawn_session", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/stats");
		});

		it("maps load_config to GET /config", () => {
			const result = mapCommandToHttp("load_config", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/config");
		});

		it("maps save_config to PUT /config", () => {
			const cfg = { font_family: "JetBrains Mono" };
			const result = mapCommandToHttp("save_config", { config: cfg });
			expect(result.method).toBe("PUT");
			expect(result.path).toBe("/config");
			expect(result.body).toEqual(cfg);
		});

		it("throws for unknown commands", () => {
			expect(() => mapCommandToHttp("unknown_cmd", {})).toThrow("No HTTP mapping for command: unknown_cmd");
		});

		it("maps previously browser-unsupported commands to HTTP", () => {
			const dictation = mapCommandToHttp("start_dictation", {});
			expect(dictation.method).toBe("POST");
			expect(dictation.path).toBe("/dictation/start");

			const openInApp = mapCommandToHttp("open_in_app", { path: "/tmp/x", app: "vscode" });
			expect(openInApp.method).toBe("POST");
			expect(openInApp.path).toBe("/agents/open-in-app");
		});

		it("maps hash_password to POST /config/hash-password with transform", () => {
			const result = mapCommandToHttp("hash_password", { password: "secret" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/config/hash-password");
			expect(result.body).toEqual({ password: "secret" });
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ hash: "abc123" })).toBe("abc123");
		});

		it("maps can_spawn_session with transform", () => {
			const result = mapCommandToHttp("can_spawn_session", {});
			expect(result.transform).toBeDefined();
			expect(result.transform?.({ active_sessions: 2, max_sessions: 5 })).toBe(true);
			expect(result.transform?.({ active_sessions: 5, max_sessions: 5 })).toBe(false);
		});

		it("maps detect_agents to GET /agents", () => {
			const result = mapCommandToHttp("detect_agents", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/agents");
		});

		it("maps get_repo_info to GET /repo/info?path=", () => {
			const result = mapCommandToHttp("get_repo_info", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/info?path=%2Fmy%2Frepo");
		});

		it("maps get_git_diff to GET /repo/diff?path=", () => {
			const result = mapCommandToHttp("get_git_diff", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/diff?path=%2Fmy%2Frepo");
		});

		it("maps get_diff_stats to GET /repo/diff-stats?path=", () => {
			const result = mapCommandToHttp("get_diff_stats", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/diff-stats?path=%2Fmy%2Frepo");
		});

		it("maps get_changed_files to GET /repo/files?path=", () => {
			const result = mapCommandToHttp("get_changed_files", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/files?path=%2Fmy%2Frepo");
		});

		it("maps get_github_status to GET /repo/github?path=", () => {
			const result = mapCommandToHttp("get_github_status", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/github?path=%2Fmy%2Frepo");
		});

		it("maps get_repo_pr_statuses to GET /repo/prs?path=", () => {
			const result = mapCommandToHttp("get_repo_pr_statuses", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/prs?path=%2Fmy%2Frepo");
		});

		it("maps get_git_branches to GET /repo/branches?path=", () => {
			const result = mapCommandToHttp("get_git_branches", { path: "/my/repo" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/branches?path=%2Fmy%2Frepo");
		});

		it("maps get_ci_checks to GET /repo/ci?path=&pr_number=", () => {
			const result = mapCommandToHttp("get_ci_checks", { path: "/my/repo", prNumber: 42 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/ci?path=%2Fmy%2Frepo&pr_number=42");
		});

		it("maps search_content to GET /fs/search-content", () => {
			const result = mapCommandToHttp("search_content", {
				repoPath: "/my/repo",
				query: "hello",
				caseSensitive: true,
				useRegex: false,
				wholeWord: false,
			});
			expect(result.method).toBe("GET");
			expect(result.path).toContain("/fs/search-content");
			expect(result.path).toContain("repoPath=%2Fmy%2Frepo");
			expect(result.path).toContain("query=hello");
			expect(result.path).toContain("caseSensitive=true");
		});

		// --- Terminal grid commands ---

		it("maps terminal_scroll to POST /sessions/{id}/terminal/scroll", () => {
			const result = mapCommandToHttp("terminal_scroll", { sessionId: "s1", delta: -5 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/scroll");
			expect(result.body).toEqual({ delta: -5 });
		});

		it("maps terminal_scroll_to to POST /sessions/{id}/terminal/scroll-to", () => {
			const result = mapCommandToHttp("terminal_scroll_to", { sessionId: "s1", line: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/scroll-to");
			expect(result.body).toEqual({ line: 42 });
		});

		it("maps terminal_scroll_info to GET /sessions/{id}/terminal/scroll-info", () => {
			const result = mapCommandToHttp("terminal_scroll_info", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/scroll-info");
		});

		it("maps terminal_search to POST with transform", () => {
			const result = mapCommandToHttp("terminal_search", { sessionId: "s1", query: "foo" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/search");
			expect(result.body).toEqual({ query: "foo" });
			expect(result.transform?.({ matches: [{ row: 0, col: 1 }] })).toEqual([{ row: 0, col: 1 }]);
		});

		it("maps terminal_search_buffer to POST with transform", () => {
			const result = mapCommandToHttp("terminal_search_buffer", { sessionId: "s1", query: "bar" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/search-buffer");
			expect(result.body).toEqual({ query: "bar" });
			expect(result.transform?.({ matches: [] })).toEqual([]);
		});

		it("maps terminal_get_row_text to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_row_text", { sessionId: "s1", row: 5 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/row-text?row=5");
			expect(result.transform?.({ text: "hello" })).toBe("hello");
		});

		it("maps terminal_get_lines to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_lines", { sessionId: "s1", start: 0, end: 3 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/lines?start=0&end=3");
			expect(result.transform?.({ lines: ["a", "b"] })).toEqual(["a", "b"]);
		});

		it("maps terminal_get_cursor_line to GET with transform", () => {
			const result = mapCommandToHttp("terminal_get_cursor_line", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/cursor-line");
			expect(result.transform?.({ text: "$ " })).toBe("$ ");
		});

		it("maps terminal_hyperlink_at to GET with transform", () => {
			const result = mapCommandToHttp("terminal_hyperlink_at", { sessionId: "s1", row: 2, col: 10 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/hyperlink?row=2&col=10");
			expect(result.transform?.({ url: "https://example.com" })).toBe("https://example.com");
			expect(result.transform?.({ url: null })).toBeNull();
		});

		it("maps terminal_request_frame to POST /sessions/{id}/terminal/request-frame", () => {
			const result = mapCommandToHttp("terminal_request_frame", { sessionId: "s1" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/terminal/request-frame");
		});

		it("maps get_agent_hook_state to GET and unwraps {state}", () => {
			const result = mapCommandToHttp("get_agent_hook_state", { agentType: "claude" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/config/agents/claude/hook-instrumentation");
			expect(result.transform?.({ state: "installed" })).toBe("installed");
		});

		it("maps set_agent_hook_instrumentation to PUT with {enabled} body", () => {
			const result = mapCommandToHttp("set_agent_hook_instrumentation", { agentType: "claude", enabled: true });
			expect(result.method).toBe("PUT");
			expect(result.path).toBe("/config/agents/claude/hook-instrumentation");
			expect(result.body).toEqual({ enabled: true });
		});

		it("maps read_plugin_data to GET /api/plugins/{id}/data/{path} with notFoundAsNull", () => {
			const result = mapCommandToHttp("read_plugin_data", {
				pluginId: "my-plugin",
				path: "credential-consent-anthropic",
			});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/api/plugins/my-plugin/data/credential-consent-anthropic");
			expect(result.notFoundAsNull).toBe(true);
			// Faithful Option<String> bridge: plain strings pass through, non-strings stringify, null stays null.
			expect(result.transform?.("allowed")).toBe("allowed");
			expect(result.transform?.({ a: 1 })).toBe('{"a":1}');
			expect(result.transform?.(null)).toBeNull();
		});

		it("maps write_plugin_data to POST with content body", () => {
			const result = mapCommandToHttp("write_plugin_data", {
				pluginId: "my-plugin",
				path: "credential-consent-anthropic",
				content: "allowed",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/api/plugins/my-plugin/data/credential-consent-anthropic");
			expect(result.body).toEqual({ content: "allowed" });
		});

		it("maps resolve_terminal_path to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("resolve_terminal_path", { cwd: "/repo", candidate: "src/x.ts" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/fs/resolve-terminal-path?cwd=%2Frepo&candidate=src%2Fx.ts");
			expect(result.transform?.({ absolute_path: "/repo/src/x.ts", is_directory: false })).toEqual({
				absolute_path: "/repo/src/x.ts",
				is_directory: false,
			});
			expect(result.transform?.(null)).toBeNull();
		});

		it("maps stat_path to GET /fs/stat?path=", () => {
			const result = mapCommandToHttp("stat_path", { path: "/repo/file.md" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/fs/stat?path=%2Frepo%2Ffile.md");
		});

		it("maps warm_content_index to POST /fs/warm-index", () => {
			const result = mapCommandToHttp("warm_content_index", { repoPath: "/repo" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/warm-index");
			expect(result.body).toEqual({ repoPath: "/repo" });
		});

		it("maps write_external_file to POST /fs/write-external", () => {
			const result = mapCommandToHttp("write_external_file", { path: "/repo/a.md", content: "hi" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/write-external");
			expect(result.body).toEqual({ path: "/repo/a.md", content: "hi" });
		});

		it("maps copy_path_abs to POST /fs/copy-abs", () => {
			const result = mapCommandToHttp("copy_path_abs", { from: "/a/x", to: "/b/x" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/copy-abs");
			expect(result.body).toEqual({ from: "/a/x", to: "/b/x" });
		});

		it("maps move_path_abs to POST /fs/move-abs", () => {
			const result = mapCommandToHttp("move_path_abs", { from: "/a/x", to: "/b/x" });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/move-abs");
			expect(result.body).toEqual({ from: "/a/x", to: "/b/x" });
		});

		it("maps fs_transfer_paths to POST /fs/transfer", () => {
			const result = mapCommandToHttp("fs_transfer_paths", {
				destDir: "/repo/dst",
				paths: ["/a/x", "/a/y"],
				mode: "move",
				allowRecursive: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/fs/transfer");
			expect(result.body).toEqual({
				destDir: "/repo/dst",
				paths: ["/a/x", "/a/y"],
				mode: "move",
				allowRecursive: true,
			});
		});

		// --- PTY/terminal read commands (story 062) ---
		it("maps get_shell_state to GET with {state} unwrap transform", () => {
			const result = mapCommandToHttp("get_shell_state", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/shell-state");
			expect(result.transform?.({ state: "busy" })).toBe("busy");
			expect(result.transform?.({ state: null })).toBeNull();
		});

		it("maps get_last_prompt to GET with {prompt} unwrap transform", () => {
			const result = mapCommandToHttp("get_last_prompt", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/last-prompt");
			expect(result.transform?.({ prompt: "do the thing" })).toBe("do the thing");
			expect(result.transform?.({ prompt: null })).toBeNull();
		});

		it("maps get_input_buffer_content to GET with {content} unwrap transform", () => {
			const result = mapCommandToHttp("get_input_buffer_content", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/input-buffer");
			expect(result.transform?.({ content: "ls -la" })).toBe("ls -la");
		});

		it("maps get_session_leaf_pid to GET with {pid} unwrap transform", () => {
			const result = mapCommandToHttp("get_session_leaf_pid", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/leaf-pid");
			expect(result.transform?.({ pid: 4321 })).toBe(4321);
			expect(result.transform?.({ pid: null })).toBeNull();
		});

		it("maps has_foreground_process to GET with {process} unwrap transform", () => {
			const result = mapCommandToHttp("has_foreground_process", { sessionId: "s1" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/has-foreground");
			expect(result.transform?.({ process: "htop" })).toBe("htop");
			expect(result.transform?.({ process: null })).toBeNull();
		});

		it("maps set_session_visible to POST /sessions/{id}/visible", () => {
			const result = mapCommandToHttp("set_session_visible", { sessionId: "s1", visible: false });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/sessions/s1/visible");
			expect(result.body).toEqual({ visible: false });
		});

		it("maps get_process_stats to GET /process/stats", () => {
			const result = mapCommandToHttp("get_process_stats", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/process/stats");
		});

		it("maps terminal_get_selection_text to GET with {text} unwrap transform", () => {
			const result = mapCommandToHttp("terminal_get_selection_text", {
				sessionId: "s1",
				startRow: 1,
				startCol: 2,
				endRow: 3,
				endCol: 4,
			});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/selection-text?startRow=1&startCol=2&endRow=3&endCol=4");
			expect(result.transform?.({ text: "hello" })).toBe("hello");
		});

		it("maps terminal_get_logical_line to GET (tuple array, no transform)", () => {
			const result = mapCommandToHttp("terminal_get_logical_line", { sessionId: "s1", row: 7 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/logical-line?row=7");
			expect(result.transform).toBeUndefined();
		});

		it("maps terminal_hyperlink_span to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("terminal_hyperlink_span", { sessionId: "s1", row: 2, col: 5 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/sessions/s1/terminal/hyperlink-span?row=2&col=5");
			expect(result.transform?.([2, 9, "https://x.dev"])).toEqual([2, 9, "https://x.dev"]);
			expect(result.transform?.(null)).toBeNull();
		});

		// --- Claude Usage dashboard (story 063) ---
		it("maps get_claude_usage_api to GET /claude/usage", () => {
			const result = mapCommandToHttp("get_claude_usage_api", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/usage");
		});

		it("maps get_claude_project_list to GET /claude/projects", () => {
			const result = mapCommandToHttp("get_claude_project_list", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/projects");
		});

		it("maps get_claude_usage_timeline to GET with scope + days", () => {
			const result = mapCommandToHttp("get_claude_usage_timeline", { scope: "all", days: 7 });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/timeline?scope=all&days=7");
		});

		it("maps get_claude_usage_timeline omitting days when absent", () => {
			const result = mapCommandToHttp("get_claude_usage_timeline", { scope: "my-proj" });
			expect(result.path).toBe("/claude/timeline?scope=my-proj");
		});

		it("maps get_claude_session_stats to GET with scope", () => {
			const result = mapCommandToHttp("get_claude_session_stats", { scope: "current" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/claude/session-stats?scope=current");
		});

		// --- Git panel (story 064) ---
		it("maps get_gutter_changes to GET with optional scope", () => {
			const a = mapCommandToHttp("get_gutter_changes", { path: "/r", file: "a.ts", scope: "head" });
			expect(a.method).toBe("GET");
			expect(a.path).toBe("/repo/gutter-changes?path=%2Fr&file=a.ts&scope=head");
			const b = mapCommandToHttp("get_gutter_changes", { path: "/r", file: "a.ts" });
			expect(b.path).toBe("/repo/gutter-changes?path=%2Fr&file=a.ts");
		});

		it("maps get_branches_detail to GET /repo/branches-detail", () => {
			const result = mapCommandToHttp("get_branches_detail", { path: "/r" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/branches-detail?path=%2Fr");
		});

		it("maps get_recent_branches with optional limit", () => {
			expect(mapCommandToHttp("get_recent_branches", { path: "/r", limit: 5 }).path).toBe(
				"/repo/recent-branches?path=%2Fr&limit=5",
			);
			expect(mapCommandToHttp("get_recent_branches", { path: "/r" }).path).toBe("/repo/recent-branches?path=%2Fr");
		});

		it("maps get_branch_base to GET with null-passthrough transform", () => {
			const result = mapCommandToHttp("get_branch_base", { path: "/r", branchName: "feat" });
			expect(result.path).toBe("/repo/branch-base?path=%2Fr&branchName=feat");
			expect(result.transform?.("main")).toBe("main");
			expect(result.transform?.(null)).toBeNull();
		});

		it("maps check_worktree_dirty to GET", () => {
			const result = mapCommandToHttp("check_worktree_dirty", { repoPath: "/r", branchName: "feat" });
			expect(result.path).toBe("/repo/worktree-dirty?repoPath=%2Fr&branchName=feat");
		});

		it("maps list_base_ref_options to GET", () => {
			expect(mapCommandToHttp("list_base_ref_options", { repoPath: "/r" }).path).toBe(
				"/repo/base-ref-options?repoPath=%2Fr",
			);
		});

		it("maps generate_clone_branch_name_cmd to POST", () => {
			const result = mapCommandToHttp("generate_clone_branch_name_cmd", {
				sourceBranch: "main",
				existingNames: ["a", "b"],
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/clone-branch-name");
			expect(result.body).toEqual({ sourceBranch: "main", existingNames: ["a", "b"] });
		});

		it("maps get_commit_graph with optional count", () => {
			expect(mapCommandToHttp("get_commit_graph", { path: "/r", count: 200 }).path).toBe(
				"/repo/commit-graph?path=%2Fr&count=200",
			);
			expect(mapCommandToHttp("get_commit_graph", { path: "/r" }).path).toBe("/repo/commit-graph?path=%2Fr");
		});

		it("maps create_branch to POST", () => {
			const result = mapCommandToHttp("create_branch", {
				path: "/r",
				name: "feat",
				startPoint: "main",
				checkout: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/create-branch");
			expect(result.body).toEqual({ path: "/r", name: "feat", startPoint: "main", checkout: true });
		});

		it("maps delete_branch to POST", () => {
			const result = mapCommandToHttp("delete_branch", { path: "/r", name: "feat", force: false });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/delete-branch");
			expect(result.body).toEqual({ path: "/r", name: "feat", force: false });
		});

		it("maps delete_local_branch to POST", () => {
			const result = mapCommandToHttp("delete_local_branch", {
				repoPath: "/r",
				branchName: "feat",
				keepWorktree: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/delete-local-branch");
			expect(result.body).toEqual({ repoPath: "/r", branchName: "feat", keepWorktree: true });
		});

		it("maps update_from_base to POST", () => {
			const result = mapCommandToHttp("update_from_base", {
				path: "/r",
				branchName: "feat",
				strategy: "rebase",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/update-from-base");
			expect(result.body).toEqual({ path: "/r", branchName: "feat", strategy: "rebase" });
		});

		it("maps switch_branch to POST", () => {
			const result = mapCommandToHttp("switch_branch", {
				repoPath: "/r",
				branchName: "feat",
				force: false,
				stash: true,
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/switch-branch");
			expect(result.body).toEqual({ repoPath: "/r", branchName: "feat", force: false, stash: true });
		});

		it("maps merge_and_archive_worktree to POST", () => {
			const result = mapCommandToHttp("merge_and_archive_worktree", {
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
			});
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/merge-archive-worktree");
			expect(result.body).toEqual({
				repoPath: "/r",
				branchName: "feat",
				targetBranch: "main",
				afterMerge: "archive",
			});
		});

		it("maps close_issue to POST", () => {
			const result = mapCommandToHttp("close_issue", { repoPath: "/r", issueNumber: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/issues/close");
			expect(result.body).toEqual({ repoPath: "/r", issueNumber: 42 });
		});

		it("maps reopen_issue to POST", () => {
			const result = mapCommandToHttp("reopen_issue", { repoPath: "/r", issueNumber: 42 });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/repo/issues/reopen");
			expect(result.body).toEqual({ repoPath: "/r", issueNumber: 42 });
		});

		it("maps get_github_viewer_login to GET", () => {
			const result = mapCommandToHttp("get_github_viewer_login", {});
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/github/viewer-login");
		});

		it("maps fetch_ci_failure_logs to GET with query", () => {
			const result = mapCommandToHttp("fetch_ci_failure_logs", { repoPath: "/r", branch: "feat" });
			expect(result.method).toBe("GET");
			expect(result.path).toBe("/repo/ci-failure-logs?repoPath=%2Fr&branch=feat");
		});

		it("maps github_set_pr_hide_drafts to POST", () => {
			const result = mapCommandToHttp("github_set_pr_hide_drafts", { hide: true });
			expect(result.method).toBe("POST");
			expect(result.path).toBe("/github/pr-hide-drafts");
			expect(result.body).toEqual({ hide: true });
		});

		it("maps github device-code auth flow", () => {
			expect(mapCommandToHttp("github_start_login", {}).path).toBe("/github/auth/start");
			expect(mapCommandToHttp("github_start_login", {}).method).toBe("POST");
			const poll = mapCommandToHttp("github_poll_login", { deviceCode: "abc" });
			expect(poll.method).toBe("POST");
			expect(poll.path).toBe("/github/auth/poll");
			expect(poll.body).toEqual({ deviceCode: "abc" });
			expect(mapCommandToHttp("github_logout", {}).path).toBe("/github/auth/logout");
			expect(mapCommandToHttp("github_disconnect", {}).path).toBe("/github/auth/disconnect");
			expect(mapCommandToHttp("github_auth_status", {}).path).toBe("/github/auth/status");
			expect(mapCommandToHttp("github_auth_status", {}).method).toBe("GET");
			expect(mapCommandToHttp("github_diagnostics", {}).path).toBe("/github/diagnostics");
		});

		it("maps ai-prompts load/save", () => {
			expect(mapCommandToHttp("load_ai_prompts", {}).path).toBe("/config/ai-prompts");
			expect(mapCommandToHttp("load_ai_prompts", {}).method).toBe("GET");
			const save = mapCommandToHttp("save_ai_prompts", { config: { a: 1 } });
			expect(save.method).toBe("PUT");
			expect(save.path).toBe("/config/ai-prompts");
			expect(save.body).toEqual({ a: 1 });
		});

		it("maps note asset commands", () => {
			const img = mapCommandToHttp("save_note_image", {
				noteId: "n1",
				dataBase64: "AAA",
				extension: "png",
			});
			expect(img.path).toBe("/config/note-image");
			expect(img.body).toEqual({ noteId: "n1", dataBase64: "AAA", extension: "png" });
			expect(mapCommandToHttp("delete_note_assets", { noteId: "n1" }).path).toBe(
				"/config/note-assets/delete",
			);
			const batch = mapCommandToHttp("delete_note_assets_batch", { noteIds: ["a", "b"] });
			expect(batch.path).toBe("/config/note-assets/delete-batch");
			expect(batch.body).toEqual({ noteIds: ["a", "b"] });
		});

		it("maps config/themes/mcp-upstreams commands", () => {
			expect(mapCommandToHttp("list_themes", {}).path).toBe("/config/themes");
			const rlc = mapCommandToHttp("save_repo_local_config", { repoPath: "/r" });
			expect(rlc.method).toBe("POST");
			expect(rlc.body).toEqual({ repoPath: "/r" });
			const bl = mapCommandToHttp("set_branch_label", {
				repoPath: "/r",
				branchName: "feat",
				label: "x",
			});
			expect(bl.path).toBe("/config/branch-label");
			expect(bl.body).toEqual({ repoPath: "/r", branchName: "feat", label: "x" });
			const up = mapCommandToHttp("set_project_mcp_upstreams", {
				repoPath: "/r",
				upstreamNames: ["a"],
			});
			expect(up.path).toBe("/config/project-mcp-upstreams");
			expect(up.body).toEqual({ repoPath: "/r", upstreamNames: ["a"] });
		});

		it("maps misc command parity (shell/audio/agent/generators/registry)", () => {
			const sh = mapCommandToHttp("execute_shell_script", {
				scriptContent: "echo hi",
				timeoutMs: 5000,
				repoPath: "/r",
			});
			expect(sh.method).toBe("POST");
			expect(sh.path).toBe("/exec/shell-script");
			expect(sh.body).toEqual({ scriptContent: "echo hi", timeoutMs: 5000, repoPath: "/r" });
			expect(mapCommandToHttp("list_audio_output_devices", {}).path).toBe("/audio/output-devices");
			const disc = mapCommandToHttp("discover_agent_session", {
				agentType: "claude",
				cwd: "/r",
				claimedIds: [],
				agentPid: 123,
				envOverrides: {},
			});
			expect(disc.path).toBe("/agent/discover-session");
			expect(disc.body).toEqual({
				agentType: "claude",
				cwd: "/r",
				claimedIds: [],
				agentPid: 123,
				envOverrides: {},
			});
			expect(mapCommandToHttp("claude_project_dir", { cwd: "/r", claudeConfigDir: null }).path).toBe(
				"/agent/claude-project-dir",
			);
			const oic = mapCommandToHttp("open_in_custom", {
				executable: "code",
				args: ["-g"],
				ctx: { repo: "/r" },
			});
			expect(oic.path).toBe("/agent/open-in-custom");
			expect(oic.body).toEqual({ executable: "code", args: ["-g"], ctx: { repo: "/r" } });
			const gen = mapCommandToHttp("generate_value", { request: { type: "password" } });
			expect(gen.path).toBe("/generators/generate");
			expect(gen.body).toEqual({ request: { type: "password" } });
			expect(mapCommandToHttp("fetch_plugin_registry", {}).path).toBe("/registry/plugins");
		});

		it("maps AI watcher CRUD (story 070)", () => {
			expect(mapCommandToHttp("watcher_list", {}).path).toBe("/ai/watchers");
			expect(mapCommandToHttp("watcher_list", {}).method).toBe("GET");
			const create = mapCommandToHttp("watcher_create", {
				name: "w1",
				sessionId: "s1",
				trigger: { type: "Idle" },
				instructions: "do it",
				promptId: null,
				repoPath: "/r",
				maxFires: 3,
				cooldownSecs: 30,
			});
			expect(create.method).toBe("POST");
			expect(create.path).toBe("/ai/watchers");
			expect(create.body).toEqual({
				name: "w1",
				sessionId: "s1",
				trigger: { type: "Idle" },
				instructions: "do it",
				promptId: null,
				repoPath: "/r",
				maxFires: 3,
				cooldownSecs: 30,
			});
			expect(mapCommandToHttp("watcher_update", { id: "x" }).path).toBe("/ai/watchers/update");
			expect(mapCommandToHttp("watcher_delete", { id: "x" }).body).toEqual({ id: "x" });
			expect(mapCommandToHttp("watcher_toggle", { id: "x", enabled: true }).body).toEqual({
				id: "x",
				enabled: true,
			});
			expect(mapCommandToHttp("watcher_attach", { templateId: "t", sessionId: "s" }).body).toEqual({
				templateId: "t",
				sessionId: "s",
			});
			expect(mapCommandToHttp("watcher_detach", { id: "x" }).path).toBe("/ai/watchers/detach");
		});
	});

	describe("INTENTIONALLY_UNMAPPED (native/host-only commands)", () => {
		it("raises a precise native-only error, not a generic missing-mapping error", () => {
			for (const command of INTENTIONALLY_UNMAPPED) {
				expect(() => mapCommandToHttp(command, {})).toThrow(/native\/host-only/);
			}
		});

		it("covers the documented native-only command families", () => {
			// Sentinels from each group in the story 073 spec.
			for (const cmd of [
				"open_panel_window",
				"start_native_drag",
				"block_sleep",
				"set_global_hotkey",
				"check_microphone_permission",
				"get_connect_url",
				"regenerate_session_token",
				"get_tailscale_status",
				"mcp_oauth_callback",
				"install_cli",
				"set_last_seen_version",
				"install_mdkb",
				"subscribe_terminal_grid",
				"ack_terminal_frame",
			]) {
				expect(INTENTIONALLY_UNMAPPED.has(cmd)).toBe(true);
			}
		});

		it("does not also have a COMMAND_TABLE mapping (would be contradictory)", () => {
			// If a command were both mapped and listed unmapped, mapCommandToHttp would
			// succeed and the native-only error would be dead. Guard against that drift.
			for (const command of INTENTIONALLY_UNMAPPED) {
				let mapped = true;
				try {
					mapCommandToHttp(command, {});
				} catch {
					mapped = false;
				}
				expect(mapped).toBe(false);
			}
		});
	});

	describe("rpc()", () => {
		const originalFetch = globalThis.fetch;
		const originalTauri = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		beforeEach(() => {
			// Ensure non-Tauri mode for HTTP tests
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		});

		afterEach(() => {
			globalThis.fetch = originalFetch;
			if (originalTauri !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauri;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("uses fetch in non-Tauri mode with JSON response", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				json: vi.fn().mockResolvedValue({ sessions: [] }),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<{ sessions: unknown[] }>("list_active_sessions");
			expect(result).toEqual({ sessions: [] });
			expect(globalThis.fetch).toHaveBeenCalledWith(
				expect.stringContaining("/sessions"),
				expect.objectContaining({ method: "GET" }),
			);
		});

		it("sends body for POST requests", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				json: vi.fn().mockResolvedValue({ id: "sess-1" }),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await rpc("create_pty", { config: { rows: 24, cols: 80, shell: null, cwd: "/tmp" } });
			const fetchCall = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
			expect(fetchCall[1].body).toBeDefined();
			expect(JSON.parse(fetchCall[1].body)).toEqual({ rows: 24, cols: 80, shell: null, cwd: "/tmp" });
		});

		it("handles text response without content-type as JSON fallback", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({}),
				text: vi.fn().mockResolvedValue('{"result":"ok"}'),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc("get_orchestrator_stats");
			expect(result).toEqual({ result: "ok" });
		});

		it("returns plain text when response is not JSON", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({}),
				text: vi.fn().mockResolvedValue("plain text response"),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc("get_orchestrator_stats");
			expect(result).toBe("plain text response");
		});

		it("throws on non-ok response", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 500,
				statusText: "Internal Server Error",
				text: vi.fn().mockResolvedValue("Something went wrong"),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("get_orchestrator_stats")).rejects.toThrow("RPC get_orchestrator_stats failed: 500");
		});

		it("applies transform when present", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: true,
				headers: new Headers({ "content-type": "application/json" }),
				json: vi.fn().mockResolvedValue({ active_sessions: 2, max_sessions: 5 }),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<boolean>("can_spawn_session");
			expect(result).toBe(true);
		});

		it("returns null on 404 when notFoundAsNull is set (read_plugin_data)", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 404,
				statusText: "Not Found",
				text: vi.fn().mockResolvedValue(""),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			const result = await rpc<string | null>("read_plugin_data", { pluginId: "p", path: "missing-key" });
			expect(result).toBeNull();
		});

		it("still throws on non-404 errors even with notFoundAsNull", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 400,
				statusText: "Bad Request",
				text: vi.fn().mockResolvedValue("bad path"),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("read_plugin_data", { pluginId: "p", path: "../escape" })).rejects.toThrow(
				"RPC read_plugin_data failed: 400",
			);
		});

		it("handles resp.text() failure in error path", async () => {
			const { rpc } = await import("../transport");

			const mockResponse = {
				ok: false,
				status: 502,
				statusText: "Bad Gateway",
				text: vi.fn().mockRejectedValue(new Error("read failed")),
			};
			globalThis.fetch = vi.fn().mockResolvedValue(mockResponse);

			await expect(rpc("get_orchestrator_stats")).rejects.toThrow("Bad Gateway");
		});
	});

	describe("subscribePty()", () => {
		const originalTauri = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		beforeEach(() => {
			// Ensure non-Tauri mode for WebSocket tests
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		});

		afterEach(() => {
			if (originalTauri !== undefined) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = originalTauri;
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		});

		it("creates WebSocket in browser mode and subscribes to events", async () => {
			const { subscribePty } = await import("../transport");

			let wsInstance: {
				onopen: (() => void) | null;
				onmessage: ((event: { data: string }) => void) | null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null;
				onerror: ((e: unknown) => void) | null;
				close: () => void;
			};

			class MockWebSocket {
				onopen: (() => void) | null = null;
				onmessage: ((event: { data: string }) => void) | null = null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
				onerror: ((e: unknown) => void) | null = null;
				close = vi.fn();
				constructor() {
					wsInstance = this;
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			const onData = vi.fn();
			const onExit = vi.fn();

			const subscribePromise = subscribePty("sess-1", onData, onExit);

			// Trigger onopen to resolve
			wsInstance!.onopen!();
			const unsub = await subscribePromise;

			// Simulate data
			wsInstance!.onmessage!({ data: "hello" });
			expect(onData).toHaveBeenCalledWith("hello");

			// Simulate clean close
			wsInstance!.onclose!({ wasClean: true, code: 1000, reason: "" });
			expect(onExit).toHaveBeenCalled();

			// Unsubscribe closes WS
			unsub();
			expect(wsInstance!.close).toHaveBeenCalled();

			globalThis.WebSocket = origWs;
		});

		it("logs warning and schedules reconnect on abnormal WebSocket close", async () => {
			const { subscribePty } = await import("../transport");

			let wsInstance: {
				onopen: (() => void) | null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null;
				onmessage: unknown;
				onerror: unknown;
				close: () => void;
			};

			class MockWebSocket {
				onopen: (() => void) | null = null;
				onmessage: unknown = null;
				onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
				onerror: unknown = null;
				close = vi.fn();
				constructor() {
					wsInstance = this;
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
			const onExit = vi.fn();

			const subscribePromise = subscribePty("sess-1", vi.fn(), onExit);
			wsInstance!.onopen!();
			const unsub = await subscribePromise;

			// Abnormal close triggers reconnect, not onExit
			wsInstance!.onclose!({ wasClean: false, code: 1006, reason: "" });
			expect(debugSpy).toHaveBeenCalledWith("[network]", expect.stringContaining("abnormally"), expect.anything());
			// onExit is NOT called on abnormal close — the transport schedules a reconnect instead
			expect(onExit).not.toHaveBeenCalled();

			unsub();
			debugSpy.mockRestore();
			globalThis.WebSocket = origWs;
		});

		it("log mode reconnect resumes from the tracked cursor, not the mount offset", async () => {
			const { subscribePty } = await import("../transport");
			vi.useFakeTimers();

			const instances: {
				url: string;
				onopen: (() => void) | null;
				onmessage: ((e: { data: string }) => void) | null;
				onclose: ((e: { code: number; reason?: string }) => void) | null;
				onerror: unknown;
				close: () => void;
			}[] = [];

			class MockWebSocket {
				url: string;
				onopen: (() => void) | null = null;
				onmessage: ((e: { data: string }) => void) | null = null;
				onclose: ((e: { code: number; reason?: string }) => void) | null = null;
				onerror: unknown = null;
				close = vi.fn();
				constructor(url: string) {
					this.url = url;
					instances.push(this as never);
				}
			}

			const origWs = globalThis.WebSocket;
			globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

			// Mount in log mode with the HTTP-fetched offset (50).
			const subscribePromise = subscribePty("sess-1", vi.fn(), vi.fn(), { format: "log", logOffset: 50 });
			instances[0].onopen?.();
			const unsub = await subscribePromise;
			expect(instances[0].url).toContain("offset=50");

			// Server advances the monotonic line cursor to 80 via a log frame.
			instances[0].onmessage?.({
				data: JSON.stringify({ type: "log", lines: [{ spans: [{ text: "x" }] }], offset: 50, total_lines: 80 }),
			});

			// Abnormal close → reconnect after backoff.
			instances[0].onclose?.({ code: 1006 });
			await vi.advanceTimersByTimeAsync(1000);

			// Reconnect must resume from the consumed cursor (80), NOT replay from mount (50).
			expect(instances.length).toBe(2);
			expect(instances[1].url).toContain("offset=80");
			expect(instances[1].url).not.toContain("offset=50");

			// Complete the reconnect handshake so the in-flight connect() promise settles.
			// (A real browser WebSocket fires onclose on close(); the mock does not, so an
			// unsettled connect() promise would otherwise leak past the test.)
			instances[1].onopen?.();

			unsub();
			globalThis.WebSocket = origWs;
			vi.useRealTimers();
		});
	});
});
