import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { isTauri, buildHttpUrl, mapCommandToHttp } from "../transport";

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
      expect(result.transform!({ agent: "claude" })).toBe("claude");
      expect(result.transform!({ agent: null })).toBeNull();
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

    it("maps hash_password to POST /config/hash-password with transform", () => {
      const result = mapCommandToHttp("hash_password", { password: "secret" });
      expect(result.method).toBe("POST");
      expect(result.path).toBe("/config/hash-password");
      expect(result.body).toEqual({ password: "secret" });
      expect(result.transform).toBeDefined();
      expect(result.transform!({ hash: "abc123" })).toBe("abc123");
    });

    it("maps can_spawn_session with transform", () => {
      const result = mapCommandToHttp("can_spawn_session", {});
      expect(result.transform).toBeDefined();
      expect(result.transform!({ active_sessions: 2, max_sessions: 5 })).toBe(true);
      expect(result.transform!({ active_sessions: 5, max_sessions: 5 })).toBe(false);
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

    it("maps get_ci_checks to GET /repo/ci?path=", () => {
      const result = mapCommandToHttp("get_ci_checks", { path: "/my/repo" });
      expect(result.method).toBe("GET");
      expect(result.path).toBe("/repo/ci?path=%2Fmy%2Frepo");
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

      let wsInstance: { onopen: (() => void) | null; onmessage: ((event: { data: string }) => void) | null; onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null; onerror: ((e: unknown) => void) | null; close: () => void };

      class MockWebSocket {
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
        onerror: ((e: unknown) => void) | null = null;
        close = vi.fn();
        constructor() { wsInstance = this; }
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

    it("logs warning on abnormal WebSocket close", async () => {
      const { subscribePty } = await import("../transport");

      let wsInstance: { onopen: (() => void) | null; onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null; onmessage: unknown; onerror: unknown; close: () => void };

      class MockWebSocket {
        onopen: (() => void) | null = null;
        onmessage: unknown = null;
        onclose: ((event: { wasClean: boolean; code: number; reason: string }) => void) | null = null;
        onerror: unknown = null;
        close = vi.fn();
        constructor() { wsInstance = this; }
      }

      const origWs = globalThis.WebSocket;
      globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      const onExit = vi.fn();

      const subscribePromise = subscribePty("sess-1", vi.fn(), onExit);
      wsInstance!.onopen!();
      await subscribePromise;

      // Abnormal close
      wsInstance!.onclose!({ wasClean: false, code: 1006, reason: "" });
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("abnormally"));
      expect(onExit).toHaveBeenCalled();

      warnSpy.mockRestore();
      globalThis.WebSocket = origWs;
    });
  });
});
