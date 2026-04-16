import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockListen = vi.fn().mockResolvedValue(() => {});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

const MOCK_CONFIG = {
  servers: [
    {
      id: "id-alpha",
      name: "alpha",
      transport: { type: "http", url: "http://localhost:8080/mcp" },
      enabled: true,
      timeout_secs: 30,
    },
    {
      id: "id-beta",
      name: "beta",
      transport: { type: "stdio", command: "npx", args: ["-y", "server"] },
      enabled: false,
      timeout_secs: 30,
    },
  ],
};

const MOCK_STATUS = {
  upstreams: [
    { name: "alpha", status: "ready", transport: { type: "http" }, tool_count: 5 },
    { name: "beta", status: "disabled", transport: { type: "stdio" }, tool_count: 0 },
  ],
};

describe("mcpPopupStore", () => {
  let store: typeof import("../../stores/mcpPopup").mcpPopupStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    mockListen.mockReset().mockResolvedValue(() => {});

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));
    vi.doMock("@tauri-apps/api/event", () => ({
      listen: mockListen,
    }));

    store = (await import("../../stores/mcpPopup")).mcpPopupStore;
  });

  describe("open/close/toggle", () => {
    it("defaults to closed with empty arrays", () => {
      testInScope(() => {
        expect(store.state.isOpen).toBe(false);
        expect(store.state.servers).toEqual([]);
        expect(store.state.status).toEqual([]);
        expect(store.state.saving).toBe(false);
      });
    });

    it("open() sets isOpen and triggers loadConfig", () => {
      testInScope(() => {
        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          return Promise.resolve(undefined);
        });

        store.open();
        expect(store.state.isOpen).toBe(true);
        // loadConfig called — both RPCs invoked
        expect(mockInvoke).toHaveBeenCalledWith("load_mcp_upstreams");
        expect(mockInvoke).toHaveBeenCalledWith("get_mcp_upstream_status");
      });
    });

    it("close() sets isOpen to false", () => {
      testInScope(() => {
        store.open();
        store.close();
        expect(store.state.isOpen).toBe(false);
      });
    });

    it("toggle() alternates isOpen", () => {
      testInScope(() => {
        store.toggle();
        expect(store.state.isOpen).toBe(true);
        store.toggle();
        expect(store.state.isOpen).toBe(false);
      });
    });
  });

  describe("loadConfig", () => {
    it("populates servers and status from RPCs", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          return Promise.resolve(undefined);
        });

        await store.loadConfig();

        expect(store.state.servers).toHaveLength(2);
        expect(store.state.servers[0].name).toBe("alpha");
        expect(store.state.servers[0].enabled).toBe(true);
        expect(store.state.servers[1].name).toBe("beta");
        expect(store.state.servers[1].enabled).toBe(false);
        expect(store.state.status).toHaveLength(2);
      });
    });

    it("handles RPC failure gracefully", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockRejectedValue(new Error("connection failed"));

        await store.loadConfig();

        // Should not throw, arrays stay empty
        expect(store.state.servers).toEqual([]);
        expect(store.state.status).toEqual([]);
      });
    });
  });

  describe("toggleServer", () => {
    it("calls save_mcp_upstreams with patched enabled field", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          if (cmd === "save_mcp_upstreams") return Promise.resolve(undefined);
          return Promise.resolve(undefined);
        });

        await store.loadConfig();
        await store.toggleServer("alpha");

        // save_mcp_upstreams called with alpha.enabled = false
        const saveCall = mockInvoke.mock.calls.find(
          (c: unknown[]) => c[0] === "save_mcp_upstreams",
        );
        expect(saveCall).toBeDefined();
        const savedConfig = (saveCall![1] as { config: { servers: { name: string; enabled: boolean }[] } }).config;
        const alphaSaved = savedConfig.servers.find(
          (s: { name: string }) => s.name === "alpha",
        );
        expect(alphaSaved?.enabled).toBe(false);

        // beta should remain unchanged
        const betaSaved = savedConfig.servers.find(
          (s: { name: string }) => s.name === "beta",
        );
        expect(betaSaved?.enabled).toBe(false);
      });
    });

    it("optimistically updates local state", async () => {
      await testInScopeAsync(async () => {
        // Make save_mcp_upstreams hang so we can check intermediate state
        let resolveSave!: () => void;
        const savePromise = new Promise<void>((r) => { resolveSave = r; });

        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          if (cmd === "save_mcp_upstreams") return savePromise;
          return Promise.resolve(undefined);
        });

        await store.loadConfig();

        // Start toggle but don't await
        const togglePromise = store.toggleServer("alpha");

        // Optimistic: alpha should be disabled immediately
        expect(store.state.servers.find((s) => s.name === "alpha")?.enabled).toBe(false);
        expect(store.state.saving).toBe(true);

        resolveSave();
        await togglePromise;

        expect(store.state.saving).toBe(false);
      });
    });

    it("rolls back on save failure", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          if (cmd === "save_mcp_upstreams") return Promise.reject(new Error("validation error"));
          return Promise.resolve(undefined);
        });

        await store.loadConfig();
        await store.toggleServer("alpha");

        // Should roll back — alpha stays enabled
        expect(store.state.servers.find((s) => s.name === "alpha")?.enabled).toBe(true);
        expect(store.state.saving).toBe(false);
      });
    });

    it("ignores toggle for unknown server name", async () => {
      await testInScopeAsync(async () => {
        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          return Promise.resolve(undefined);
        });

        await store.loadConfig();
        await store.toggleServer("nonexistent");

        // save_mcp_upstreams should NOT have been called
        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_mcp_upstreams",
        );
        expect(saveCalls).toHaveLength(0);
      });
    });

    it("ignores concurrent toggles while saving", async () => {
      await testInScopeAsync(async () => {
        let resolveSave!: () => void;
        const savePromise = new Promise<void>((r) => { resolveSave = r; });

        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(MOCK_STATUS);
          if (cmd === "save_mcp_upstreams") return savePromise;
          return Promise.resolve(undefined);
        });

        await store.loadConfig();

        // First toggle
        const p1 = store.toggleServer("alpha");

        // Second toggle while first is in flight — should be ignored
        const p2 = store.toggleServer("beta");

        resolveSave();
        await p1;
        await p2;

        // Only one save call
        const saveCalls = mockInvoke.mock.calls.filter(
          (c: unknown[]) => c[0] === "save_mcp_upstreams",
        );
        expect(saveCalls).toHaveLength(1);
      });
    });
  });

  describe("refreshStatus", () => {
    it("updates status without touching servers", async () => {
      await testInScopeAsync(async () => {
        const updatedStatus = {
          upstreams: [
            { name: "alpha", status: "failed", transport: { type: "http" }, tool_count: 0 },
          ],
        };

        mockInvoke.mockImplementation((cmd: string) => {
          if (cmd === "load_mcp_upstreams") return Promise.resolve(MOCK_CONFIG);
          if (cmd === "get_mcp_upstream_status") return Promise.resolve(updatedStatus);
          return Promise.resolve(undefined);
        });

        await store.loadConfig();

        // Servers should be from config, not status
        expect(store.state.servers).toHaveLength(2);
        expect(store.state.status).toHaveLength(1);
        expect(store.state.status[0].status).toBe("failed");
      });
    });
  });

  describe("listenForStatusChanges", () => {
    it("registers upstream-status-changed listener", async () => {
      await testInScopeAsync(async () => {
        const unlisten = await store.listenForStatusChanges();

        expect(mockListen).toHaveBeenCalledWith(
          "upstream-status-changed",
          expect.any(Function),
        );

        expect(typeof unlisten).toBe("function");
      });
    });
  });
});
