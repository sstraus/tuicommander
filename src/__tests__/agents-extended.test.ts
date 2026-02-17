import { describe, it, expect, beforeEach } from "vitest";
import "./mocks/tauri";
import { mockInvoke } from "./mocks/tauri";
import { AgentManager } from "../agents";

describe("AgentManager async methods", () => {
  let manager: AgentManager;

  beforeEach(() => {
    manager = new AgentManager();
    mockInvoke.mockReset();
  });

  describe("detectAgent()", () => {
    it("calls invoke('detect_agent_binary') on first call", async () => {
      mockInvoke.mockResolvedValueOnce({ path: "/usr/bin/claude", version: "1.0.0" });

      await manager.detectAgent("claude");

      expect(mockInvoke).toHaveBeenCalledWith("detect_agent_binary", { binary: "claude" });
    });

    it("returns available=true when path is not null", async () => {
      mockInvoke.mockResolvedValueOnce({ path: "/usr/bin/claude", version: "1.0.0" });

      const result = await manager.detectAgent("claude");

      expect(result.available).toBe(true);
      expect(result.path).toBe("/usr/bin/claude");
      expect(result.version).toBe("1.0.0");
      expect(result.type).toBe("claude");
    });

    it("returns available=false when invoke throws", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("binary not found"));

      const result = await manager.detectAgent("claude");

      expect(result.available).toBe(false);
      expect(result.path).toBeNull();
      expect(result.version).toBeNull();
    });

    it("returns cached result on second call without invoking again", async () => {
      mockInvoke.mockResolvedValueOnce({ path: "/usr/bin/claude", version: "1.0.0" });

      const first = await manager.detectAgent("claude");
      const second = await manager.detectAgent("claude");

      // invoke should only be called once
      expect(mockInvoke).toHaveBeenCalledTimes(1);
      expect(second).toEqual(first);
    });
  });

  describe("detectAllAgents()", () => {
    it("detects all 5 agent types", async () => {
      // Mock invoke to return a path for each agent binary
      mockInvoke.mockImplementation((_cmd: string, args: { binary: string }) => {
        const paths: Record<string, string> = {
          claude: "/usr/bin/claude",
          gemini: "/usr/bin/gemini",
          opencode: "/usr/bin/opencode",
          aider: "/usr/bin/aider",
          codex: "/usr/bin/codex",
        };
        return Promise.resolve({
          path: paths[args.binary] ?? null,
          version: "1.0.0",
        });
      });

      const results = await manager.detectAllAgents();

      expect(results).toHaveLength(5);
      const types = results.map((r) => r.type);
      expect(types).toContain("claude");
      expect(types).toContain("gemini");
      expect(types).toContain("opencode");
      expect(types).toContain("aider");
      expect(types).toContain("codex");

      // All should be available since we provided paths
      for (const result of results) {
        expect(result.available).toBe(true);
      }
    });
  });

  describe("getAvailableAgents()", () => {
    it("returns only agents with available=true", async () => {
      mockInvoke.mockImplementation((_cmd: string, args: { binary: string }) => {
        // Only claude and aider are available
        if (args.binary === "claude" || args.binary === "aider") {
          return Promise.resolve({ path: `/usr/bin/${args.binary}`, version: "1.0.0" });
        }
        return Promise.resolve({ path: null, version: null });
      });

      const available = await manager.getAvailableAgents();

      expect(available.length).toBe(2);
      const types = available.map((r) => r.type);
      expect(types).toContain("claude");
      expect(types).toContain("aider");
      expect(types).not.toContain("gemini");
    });
  });

  describe("findNextAvailableAgent()", () => {
    it("returns next available agent excluding specified types", async () => {
      mockInvoke.mockImplementation((_cmd: string, args: { binary: string }) => {
        if (args.binary === "claude" || args.binary === "gemini") {
          return Promise.resolve({ path: `/usr/bin/${args.binary}`, version: "1.0.0" });
        }
        return Promise.resolve({ path: null, version: null });
      });

      const next = await manager.findNextAvailableAgent(["claude"]);

      expect(next).toBe("gemini");
    });

    it("skips rate-limited agents", async () => {
      mockInvoke.mockImplementation((_cmd: string, args: { binary: string }) => {
        if (args.binary === "claude" || args.binary === "gemini" || args.binary === "aider") {
          return Promise.resolve({ path: `/usr/bin/${args.binary}`, version: "1.0.0" });
        }
        return Promise.resolve({ path: null, version: null });
      });

      // Mark gemini as rate-limited
      manager.markRateLimited("gemini");

      // Exclude claude, gemini is rate-limited, so should get aider
      const next = await manager.findNextAvailableAgent(["claude"]);

      expect(next).toBe("aider");
    });

    it("returns null when no agents are available", async () => {
      // All invocations return no path
      mockInvoke.mockResolvedValue({ path: null, version: null });

      const next = await manager.findNextAvailableAgent([]);

      expect(next).toBeNull();
    });
  });

  describe("clearCache()", () => {
    it("clears detection cache so next detect calls invoke again", async () => {
      mockInvoke.mockResolvedValue({ path: "/usr/bin/claude", version: "1.0.0" });

      // First detection - should call invoke
      await manager.detectAgent("claude");
      expect(mockInvoke).toHaveBeenCalledTimes(1);

      // Second detection - should use cache (no additional invoke)
      await manager.detectAgent("claude");
      expect(mockInvoke).toHaveBeenCalledTimes(1);

      // Clear cache
      manager.clearCache();

      // Third detection - cache cleared, should call invoke again
      await manager.detectAgent("claude");
      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });
  });
});
