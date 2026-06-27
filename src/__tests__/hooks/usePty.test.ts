import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { browserCreatedSessions } from "../../hooks/useAppInit";
import { usePty } from "../../hooks/usePty";
import { mockInvoke } from "../mocks/tauri";

describe("usePty", () => {
	let pty: ReturnType<typeof usePty>;

	beforeEach(() => {
		mockInvoke.mockReset();
		pty = usePty();
	});

	describe("canSpawn()", () => {
		it("returns true when invoke resolves true", async () => {
			mockInvoke.mockResolvedValueOnce(true);
			const result = await pty.canSpawn();
			expect(result).toBe(true);
			expect(mockInvoke).toHaveBeenCalledWith("can_spawn_session");
		});

		it("returns false when invoke resolves false", async () => {
			mockInvoke.mockResolvedValueOnce(false);
			const result = await pty.canSpawn();
			expect(result).toBe(false);
		});

		it("returns false on error", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("limit check failed"));
			const result = await pty.canSpawn();
			expect(result).toBe(false);
		});
	});

	describe("createSession()", () => {
		it("calls invoke with config and returns session ID", async () => {
			const config = { cwd: "/tmp", rows: 24, cols: 80, shell: null };
			mockInvoke.mockResolvedValueOnce("sess-abc");
			const result = await pty.createSession(config);
			expect(result).toBe("sess-abc");
			expect(mockInvoke).toHaveBeenCalledWith("create_pty", { config });
		});
	});

	describe("createSession() browser mode", () => {
		let fetchMock: ReturnType<typeof vi.fn>;
		const realFetch = globalThis.fetch;

		beforeEach(() => {
			// Force browser mode: isTauri() is false when __TAURI_SHIM__ is set.
			(globalThis as Record<string, unknown>).__TAURI_SHIM__ = true;
			fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
				const body = JSON.parse((init?.body as string) ?? "{}");
				// Echo back the client-provided id (mirroring the backend honoring it);
				// superset also covers the worktree result shape.
				const sessionId = body.session_id ?? body.config?.session_id ?? "backend-id";
				return new Response(JSON.stringify({ session_id: sessionId, worktree_path: "/wt/feat-x", branch: "feat-x" }), {
					status: 201,
					headers: { "content-type": "application/json" },
				});
			});
			globalThis.fetch = fetchMock as unknown as typeof fetch;
		});

		afterEach(() => {
			delete (globalThis as Record<string, unknown>).__TAURI_SHIM__;
			globalThis.fetch = realFetch;
		});

		it("pre-registers a client-generated session id before the create RPC (no duplicate-tab echo)", async () => {
			const config = { cwd: "/tmp", rows: 24, cols: 80, shell: null };
			const sessionId = await pty.createSession(config);

			// The body sent to the backend carried a client-generated session_id...
			const sentBody = JSON.parse(fetchMock.mock.calls[0][1].body as string);
			expect(sentBody.session_id).toBeTruthy();
			expect(sessionId).toBe(sentBody.session_id);
			// ...and it was registered locally so the session-created SSE echo is
			// recognized as locally-created (suppressing the duplicate "PTY:" tab).
			expect(browserCreatedSessions.has(sessionId)).toBe(true);
		});

		it("flattens worktree_config into the create-worktree body (browser routing)", async () => {
			const ptyConfig = { cwd: "/tmp", rows: 24, cols: 80, shell: null };
			const worktreeConfig = {
				task_name: "feat-x",
				base_repo: "/repos/main",
				branch: "feat-x",
				create_branch: true,
			};
			const result = await pty.createSessionWithWorktree(ptyConfig, worktreeConfig);

			const sentBody = JSON.parse(fetchMock.mock.calls[0][1].body as string);
			// The HTTP route expects a flat { config, base_repo, branch_name } — the
			// map must flatten worktree_config (previously sent the wrong keys).
			expect(sentBody.base_repo).toBe("/repos/main");
			expect(sentBody.branch_name).toBe("feat-x");
			expect(sentBody.config.session_id).toBeTruthy();
			expect(browserCreatedSessions.has(result.session_id)).toBe(true);
		});
	});

	describe("createSessionWithWorktree()", () => {
		it("calls invoke with both configs and returns worktree result", async () => {
			const ptyConfig = { cwd: "/tmp", rows: 24, cols: 80, shell: null };
			const worktreeConfig = {
				task_name: "feature-x",
				base_repo: "/repos/main",
				branch: "feature-x",
				create_branch: true,
			};
			const expected = {
				session_id: "sess-123",
				worktree_path: "/worktrees/feature-x",
				branch: "feature-x",
			};
			mockInvoke.mockResolvedValueOnce(expected);

			const result = await pty.createSessionWithWorktree(ptyConfig, worktreeConfig);
			expect(result).toEqual(expected);
			expect(mockInvoke).toHaveBeenCalledWith("create_pty_with_worktree", {
				pty_config: ptyConfig,
				worktree_config: worktreeConfig,
			});
		});
	});

	describe("write()", () => {
		it("calls invoke with sessionId and data", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.write("sess-1", "hello\n");
			expect(mockInvoke).toHaveBeenCalledWith("write_pty", {
				sessionId: "sess-1",
				data: "hello\n",
			});
		});
	});

	describe("resize()", () => {
		it("calls invoke with sessionId, rows, and cols", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.resize("sess-1", 40, 120);
			expect(mockInvoke).toHaveBeenCalledWith("resize_pty", {
				sessionId: "sess-1",
				rows: 40,
				cols: 120,
			});
		});
	});

	describe("pause()", () => {
		it("calls invoke with correct command and sessionId", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.pause("sess-1");
			expect(mockInvoke).toHaveBeenCalledWith("pause_pty", { sessionId: "sess-1" });
		});
	});

	describe("resume()", () => {
		it("calls invoke with correct command and sessionId", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.resume("sess-1");
			expect(mockInvoke).toHaveBeenCalledWith("resume_pty", { sessionId: "sess-1" });
		});
	});

	describe("close()", () => {
		it("calls invoke with sessionId and cleanupWorktree=false by default", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.close("sess-1");
			expect(mockInvoke).toHaveBeenCalledWith("close_pty", {
				sessionId: "sess-1",
				cleanupWorktree: false,
			});
		});

		it("calls invoke with cleanupWorktree=true when specified", async () => {
			mockInvoke.mockResolvedValueOnce(undefined);
			await pty.close("sess-1", true);
			expect(mockInvoke).toHaveBeenCalledWith("close_pty", {
				sessionId: "sess-1",
				cleanupWorktree: true,
			});
		});
	});

	describe("getStats()", () => {
		it("returns stats from invoke", async () => {
			const stats = { active: 3, total: 10, maxConcurrent: 5 };
			mockInvoke.mockResolvedValueOnce(stats);
			const result = await pty.getStats();
			expect(result).toEqual(stats);
			expect(mockInvoke).toHaveBeenCalledWith("get_orchestrator_stats");
		});
	});

	describe("listWorktrees()", () => {
		it("returns array from invoke", async () => {
			const worktrees = [{ name: "feat-a", path: "/wt/feat-a" }];
			mockInvoke.mockResolvedValueOnce(worktrees);
			const result = await pty.listWorktrees();
			expect(result).toEqual(worktrees);
			expect(mockInvoke).toHaveBeenCalledWith("list_worktrees");
		});
	});

	describe("getWorktreesDir()", () => {
		it("returns string from invoke", async () => {
			mockInvoke.mockResolvedValueOnce("/home/user/.worktrees");
			const result = await pty.getWorktreesDir();
			expect(result).toBe("/home/user/.worktrees");
			expect(mockInvoke).toHaveBeenCalledWith("get_worktrees_dir", { repoPath: null });
		});
	});

	describe("getMetrics()", () => {
		it("returns metrics from invoke", async () => {
			const metrics = {
				total_spawned: 15,
				failed_spawns: 2,
				active_sessions: 5,
				bytes_emitted: 102400,
				pauses_triggered: 3,
			};
			mockInvoke.mockResolvedValueOnce(metrics);
			const result = await pty.getMetrics();
			expect(result).toEqual(metrics);
			expect(mockInvoke).toHaveBeenCalledWith("get_session_metrics");
		});
	});

	describe("listActiveSessions()", () => {
		it("returns active sessions from invoke", async () => {
			const sessions = [
				{
					session_id: "uuid-1",
					cwd: "/repos/my-project",
					worktree_path: null,
					worktree_branch: null,
				},
				{
					session_id: "uuid-2",
					cwd: "/worktrees/feature-x",
					worktree_path: "/worktrees/feature-x",
					worktree_branch: "feature-x",
				},
			];
			mockInvoke.mockResolvedValueOnce(sessions);
			const result = await pty.listActiveSessions();
			expect(result).toEqual(sessions);
			expect(mockInvoke).toHaveBeenCalledWith("list_active_sessions");
		});

		it("returns empty array when no sessions exist", async () => {
			mockInvoke.mockResolvedValueOnce([]);
			const result = await pty.listActiveSessions();
			expect(result).toEqual([]);
		});
	});
});
