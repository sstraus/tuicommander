import { describe, it, expect, beforeEach } from "vitest";
import "./mocks/tauri";
import { mockInvoke } from "./mocks/tauri";
import { AgentManager, AGENTS, AGENT_DISPLAY, type AgentType } from "../agents";

describe("AgentManager", () => {
  let manager: AgentManager;

  beforeEach(() => {
    manager = new AgentManager();
  });

  describe("getAllTypes()", () => {
    it("returns all agent types", () => {
      const types = manager.getAllTypes();
      expect(types).toContain("claude");
      expect(types).toContain("gemini");
      expect(types).toContain("opencode");
      expect(types).toContain("aider");
      expect(types).toContain("codex");
      expect(types).toHaveLength(5);
    });
  });

  describe("getConfig()", () => {
    it("returns config for each agent type", () => {
      const config = manager.getConfig("claude");
      expect(config.name).toBe("Claude Code");
      expect(config.binary).toBe("claude");
      expect(config.type).toBe("claude");
    });

    it("returns config for aider", () => {
      const config = manager.getConfig("aider");
      expect(config.name).toBe("Aider");
      expect(config.binary).toBe("aider");
    });
  });

  describe("active agent", () => {
    it("defaults to claude", () => {
      expect(manager.getActiveAgent()).toBe("claude");
    });

    it("can set active agent", () => {
      manager.setActiveAgent("gemini");
      expect(manager.getActiveAgent()).toBe("gemini");
    });
  });

  describe("rate limiting", () => {
    it("agents are not rate-limited by default", () => {
      expect(manager.isRateLimited("claude")).toBe(false);
    });

    it("can mark agent as rate-limited", () => {
      manager.markRateLimited("claude");
      expect(manager.isRateLimited("claude")).toBe(true);
    });

    it("can clear rate limit", () => {
      manager.markRateLimited("claude");
      manager.clearRateLimit("claude");
      expect(manager.isRateLimited("claude")).toBe(false);
    });
  });

  describe("buildSpawnCommand()", () => {
    it("builds command for claude", () => {
      const cmd = manager.buildSpawnCommand("claude", "hello");
      expect(cmd.binary).toBe("claude");
      expect(cmd.args).toContain("hello");
    });

    it("includes print mode for claude", () => {
      const cmd = manager.buildSpawnCommand("claude", "hello", { printMode: true });
      expect(cmd.args).toContain("--print");
    });

    it("includes model option for claude", () => {
      const cmd = manager.buildSpawnCommand("claude", "hello", { model: "opus" });
      expect(cmd.args).toContain("--model");
      expect(cmd.args).toContain("opus");
    });

    it("includes output format for claude", () => {
      const cmd = manager.buildSpawnCommand("claude", "hello", { outputFormat: "json" });
      expect(cmd.args).toContain("--output-format");
      expect(cmd.args).toContain("json");
    });

    it("builds command for aider with --yes-always", () => {
      const cmd = manager.buildSpawnCommand("aider", "fix the bug");
      expect(cmd.binary).toBe("aider");
      expect(cmd.args).toContain("--yes-always");
      expect(cmd.args).toContain("--message");
      expect(cmd.args).toContain("fix the bug");
    });

    it("builds command for gemini", () => {
      const cmd = manager.buildSpawnCommand("gemini", "hello", { model: "pro" });
      expect(cmd.binary).toBe("gemini");
      expect(cmd.args).toContain("--model");
      expect(cmd.args).toContain("pro");
      expect(cmd.args).toContain("hello");
    });

    it("builds command for opencode with model", () => {
      const cmd = manager.buildSpawnCommand("opencode", "hello", { model: "gpt-4" });
      expect(cmd.binary).toBe("opencode");
      expect(cmd.args).toContain("--model");
      expect(cmd.args).toContain("gpt-4");
      expect(cmd.args).toContain("hello");
    });

    it("builds command for opencode without options", () => {
      const cmd = manager.buildSpawnCommand("opencode", "hello");
      expect(cmd.binary).toBe("opencode");
      expect(cmd.args).toEqual(["hello"]);
    });

    it("builds command for codex with model", () => {
      const cmd = manager.buildSpawnCommand("codex", "test", { model: "codex-mini" });
      expect(cmd.binary).toBe("codex");
      expect(cmd.args).toContain("--model");
      expect(cmd.args).toContain("codex-mini");
    });

    it("builds command for codex without options", () => {
      const cmd = manager.buildSpawnCommand("codex", "test");
      expect(cmd.binary).toBe("codex");
      expect(cmd.args).toEqual(["test"]);
    });
  });

  describe("checkRateLimit()", () => {
    it("detects rate limit for claude", () => {
      expect(manager.checkRateLimit("claude", "Error: rate limit exceeded")).toBe(true);
      expect(manager.checkRateLimit("claude", "429 Too Many Requests")).toBe(true);
      expect(manager.checkRateLimit("claude", "server overloaded")).toBe(true);
    });

    it("returns false for non-rate-limit output", () => {
      expect(manager.checkRateLimit("claude", "Hello, world!")).toBe(false);
    });

    it("detects RESOURCE_EXHAUSTED for gemini", () => {
      expect(manager.checkRateLimit("gemini", "RESOURCE_EXHAUSTED")).toBe(true);
    });
  });

  describe("checkCompletion()", () => {
    it("detects completion for claude", () => {
      expect(manager.checkCompletion("claude", "Done (5 tool uses)")).toBe(true);
      expect(manager.checkCompletion("claude", "completed successfully")).toBe(true);
    });

    it("returns false for non-completion output", () => {
      expect(manager.checkCompletion("claude", "Working on it...")).toBe(false);
    });

    it("detects completion for aider", () => {
      expect(manager.checkCompletion("aider", "Applied edit to file.py")).toBe(true);
      expect(manager.checkCompletion("aider", "Committed abc123")).toBe(true);
    });
  });

  describe("checkError()", () => {
    it("detects errors", () => {
      expect(manager.checkError("claude", "Error: something went wrong")).toBe(true);
      expect(manager.checkError("claude", "Failed: timeout")).toBe(true);
      expect(manager.checkError("claude", "Exception: null pointer")).toBe(true);
    });

    it("returns false for non-error output", () => {
      expect(manager.checkError("claude", "All good!")).toBe(false);
    });
  });

  describe("checkPrompt()", () => {
    it("detects prompt patterns for claude", () => {
      expect(manager.checkPrompt("claude", "[y/n]")).toBe(true);
      expect(manager.checkPrompt("claude", "Select an option:")).toBe(true);
    });

    it("detects prompt patterns for aider", () => {
      expect(manager.checkPrompt("aider", "[Y/n]")).toBe(true);
      expect(manager.checkPrompt("aider", "Enter filename:")).toBe(true);
    });
  });

  describe("cache management", () => {
    it("clearCache() resets detection cache", () => {
      // Just ensure it doesn't throw
      manager.clearCache();
    });
  });

  describe("detectAgent()", () => {
    beforeEach(() => {
      mockInvoke.mockReset();
      manager.clearCache();
    });

    it("returns available result when binary is found", async () => {
      mockInvoke.mockResolvedValueOnce({ path: "/usr/bin/claude", version: "1.0" });

      const result = await manager.detectAgent("claude");
      expect(result.type).toBe("claude");
      expect(result.available).toBe(true);
      expect(result.path).toBe("/usr/bin/claude");
      expect(result.version).toBe("1.0");
    });

    it("returns unavailable result when binary is not found", async () => {
      mockInvoke.mockResolvedValueOnce({ path: null, version: null });

      const result = await manager.detectAgent("gemini");
      expect(result.type).toBe("gemini");
      expect(result.available).toBe(false);
      expect(result.path).toBeNull();
    });

    it("returns cached result on second call", async () => {
      mockInvoke.mockResolvedValueOnce({ path: "/usr/bin/claude", version: "1.0" });

      await manager.detectAgent("claude");
      const result = await manager.detectAgent("claude");

      expect(result.available).toBe(true);
      // invoke should only be called once due to caching
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });

    it("returns unavailable when invoke throws", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("binary not found"));

      const result = await manager.detectAgent("codex");
      expect(result.type).toBe("codex");
      expect(result.available).toBe(false);
      expect(result.path).toBeNull();
    });
  });

  describe("detectAllAgents()", () => {
    beforeEach(() => {
      mockInvoke.mockReset();
      manager.clearCache();
    });

    it("detects all agents", async () => {
      mockInvoke.mockResolvedValue({ path: null, version: null });

      const results = await manager.detectAllAgents();
      expect(results).toHaveLength(5);
      expect(results.map((r) => r.type)).toEqual(["claude", "gemini", "opencode", "aider", "codex"]);
    });
  });

  describe("getAvailableAgents()", () => {
    beforeEach(() => {
      mockInvoke.mockReset();
      manager.clearCache();
    });

    it("returns only available agents", async () => {
      mockInvoke.mockImplementation(async (_cmd: string, args: { binary: string }) => {
        if (args.binary === "claude") return { path: "/usr/bin/claude", version: "1.0" };
        if (args.binary === "aider") return { path: "/usr/bin/aider", version: "2.0" };
        return { path: null, version: null };
      });

      const results = await manager.getAvailableAgents();
      expect(results).toHaveLength(2);
      expect(results.map((r) => r.type)).toEqual(["claude", "aider"]);
    });
  });

  describe("findNextAvailableAgent()", () => {
    beforeEach(() => {
      mockInvoke.mockReset();
      manager.clearCache();
    });

    it("returns next available agent excluding specified ones", async () => {
      mockInvoke.mockImplementation(async (_cmd: string, args: { binary: string }) => {
        if (args.binary === "claude") return { path: "/usr/bin/claude", version: "1.0" };
        if (args.binary === "gemini") return { path: "/usr/bin/gemini", version: "1.0" };
        return { path: null, version: null };
      });

      const next = await manager.findNextAvailableAgent(["claude"]);
      expect(next).toBe("gemini");
    });

    it("excludes rate-limited agents", async () => {
      mockInvoke.mockImplementation(async (_cmd: string, args: { binary: string }) => {
        if (args.binary === "claude") return { path: "/usr/bin/claude", version: "1.0" };
        if (args.binary === "gemini") return { path: "/usr/bin/gemini", version: "1.0" };
        return { path: null, version: null };
      });

      manager.markRateLimited("gemini");
      const next = await manager.findNextAvailableAgent(["claude"]);
      expect(next).toBeNull(); // gemini is available but rate-limited
    });

    it("returns null when no agents available", async () => {
      mockInvoke.mockResolvedValue({ path: null, version: null });

      const next = await manager.findNextAvailableAgent([]);
      expect(next).toBeNull();
    });
  });
});

describe("AGENTS config", () => {
  it("has all required fields for each agent", () => {
    const types: AgentType[] = ["claude", "gemini", "opencode", "aider", "codex"];
    for (const type of types) {
      const config = AGENTS[type];
      expect(config.type).toBe(type);
      expect(config.name).toBeTruthy();
      expect(config.binary).toBeTruthy();
      expect(config.description).toBeTruthy();
      expect(typeof config.spawnArgs).toBe("function");
      expect(config.detectPatterns.rateLimit.length).toBeGreaterThan(0);
      expect(config.detectPatterns.completion.length).toBeGreaterThan(0);
      expect(config.detectPatterns.error.length).toBeGreaterThan(0);
      expect(config.detectPatterns.prompt.length).toBeGreaterThan(0);
    }
  });
});

describe("AGENT_DISPLAY", () => {
  it("has display info for each agent", () => {
    const types: AgentType[] = ["claude", "gemini", "opencode", "aider", "codex"];
    for (const type of types) {
      expect(AGENT_DISPLAY[type].icon).toBeTruthy();
      expect(AGENT_DISPLAY[type].color).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});
