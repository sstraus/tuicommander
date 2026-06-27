import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildHttpUrl, isTauri, mapCommandToHttp } from "../transport";

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
