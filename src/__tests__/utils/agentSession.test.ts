import { describe, it, expect, vi, beforeEach } from "vitest";
import { buildAgentLaunchCommand, buildResumeCommand, verifyAndBuildResumeCommand } from "../../utils/agentSession";
import { AGENTS } from "../../agents";

// Mock rpc for verifyAndBuildResumeCommand tests
const mockRpc = vi.fn();
vi.mock("../../transport", () => ({
  rpc: (...args: unknown[]) => mockRpc(...args),
}));

describe("buildAgentLaunchCommand", () => {
  it("injects --session-id for claude when UUID provided", () => {
    expect(buildAgentLaunchCommand("claude", "abc-123")).toBe("claude --session-id abc-123");
  });

  it("returns bare binary for claude without UUID", () => {
    expect(buildAgentLaunchCommand("claude")).toBe("claude");
  });

  it("returns bare binary for claude with null UUID", () => {
    expect(buildAgentLaunchCommand("claude", null)).toBe("claude");
  });

  it("returns bare binary for non-claude agents even with UUID", () => {
    expect(buildAgentLaunchCommand("gemini", "abc-123")).toBe("gemini");
  });

  it("injects --session-id into full command with args", () => {
    expect(buildAgentLaunchCommand("claude --model opus", "abc-123")).toBe(
      "claude --session-id abc-123 --model opus",
    );
  });

  it("handles command with path prefix", () => {
    expect(buildAgentLaunchCommand("/usr/local/bin/claude", "abc-123")).toBe(
      "/usr/local/bin/claude --session-id abc-123",
    );
  });

  it("handles command with path and args", () => {
    expect(buildAgentLaunchCommand("/usr/local/bin/claude --model sonnet", "uuid-1")).toBe(
      "/usr/local/bin/claude --session-id uuid-1 --model sonnet",
    );
  });
});

describe("buildResumeCommand", () => {
  it("returns --resume <uuid> for claude with UUID", () => {
    expect(buildResumeCommand("claude", "abc-123")).toBe("claude --resume abc-123");
  });

  it("falls back to --continue for claude without UUID", () => {
    expect(buildResumeCommand("claude", null)).toBe("claude --continue");
  });

  it("falls back to --continue for claude with undefined UUID", () => {
    expect(buildResumeCommand("claude")).toBe("claude --continue");
  });

  it("returns id-based resume for gemini with UUID", () => {
    expect(buildResumeCommand("gemini", "abc-123")).toBe("gemini --resume abc-123");
  });

  it("falls back to static resume for gemini without UUID", () => {
    expect(buildResumeCommand("gemini", null)).toBe("gemini --resume");
  });

  it("returns id-based resume for codex with UUID", () => {
    expect(buildResumeCommand("codex", "abc-123")).toBe("codex resume abc-123");
  });

  it("falls back to static resume for codex without UUID", () => {
    expect(buildResumeCommand("codex", null)).toBe("codex resume --last");
  });

  it("returns static resume for aider (no session discovery)", () => {
    expect(buildResumeCommand("aider", null)).toBe("aider --restore-chat-history");
  });

  it("returns static resume for amp", () => {
    expect(buildResumeCommand("amp", null)).toBe("amp threads continue");
  });

  it("returns null for agents without resume support", () => {
    expect(buildResumeCommand("warp", null)).toBeNull();
    expect(buildResumeCommand("droid", null)).toBeNull();
    expect(buildResumeCommand("git", null)).toBeNull();
  });
});

describe("sessionDiscovery in AgentConfig", () => {
  it("claude has sessionDiscovery with resumeWithId", () => {
    const disc = AGENTS.claude.sessionDiscovery;
    expect(disc).not.toBeNull();
    expect(disc?.resumeWithId("test-uuid")).toBe("claude --resume test-uuid");
  });

  it("gemini has sessionDiscovery with resumeWithId", () => {
    const disc = AGENTS.gemini.sessionDiscovery;
    expect(disc).not.toBeNull();
    expect(disc?.resumeWithId("test-uuid")).toBe("gemini --resume test-uuid");
  });

  it("codex has sessionDiscovery with resumeWithId", () => {
    const disc = AGENTS.codex.sessionDiscovery;
    expect(disc).not.toBeNull();
    expect(disc?.resumeWithId("test-uuid")).toBe("codex resume test-uuid");
  });

  it("aider has null sessionDiscovery (no session IDs)", () => {
    expect(AGENTS.aider.sessionDiscovery).toBeNull();
  });

  it("amp has null sessionDiscovery (cloud-only)", () => {
    expect(AGENTS.amp.sessionDiscovery).toBeNull();
  });

  it("warp has null sessionDiscovery", () => {
    expect(AGENTS.warp.sessionDiscovery).toBeNull();
  });

  it("opencode has null sessionDiscovery (SQLite, not implemented)", () => {
    expect(AGENTS.opencode.sessionDiscovery).toBeNull();
  });
});

describe("verifyAndBuildResumeCommand", () => {
  beforeEach(() => {
    mockRpc.mockReset();
  });

  it("uses tuicSession when verified on disk", async () => {
    mockRpc.mockResolvedValueOnce(true);
    const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "old-session-id");
    expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
      agentType: "claude",
      sessionId: "tuic-uuid-1",
      cwd: "/tmp/repo",
    });
    expect(result).toBe("claude --resume tuic-uuid-1");
  });

  it("falls back to agentSessionId when tuicSession not verified", async () => {
    mockRpc.mockResolvedValueOnce(false);
    const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "old-session-id");
    expect(result).toBe("claude --resume old-session-id");
  });

  it("falls back to static resume when neither session exists", async () => {
    mockRpc.mockResolvedValueOnce(false);
    const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", null);
    expect(result).toBe("claude --continue");
  });

  it("falls back gracefully when rpc throws (browser mode)", async () => {
    mockRpc.mockRejectedValueOnce(new Error("browser unsupported"));
    const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", "tuic-uuid-1", "old-session-id");
    expect(result).toBe("claude --resume old-session-id");
  });

  it("skips verification when tuicSession is null", async () => {
    const result = await verifyAndBuildResumeCommand("claude", "/tmp/repo", null, "old-session-id");
    expect(mockRpc).not.toHaveBeenCalled();
    expect(result).toBe("claude --resume old-session-id");
  });

  it("skips verification when cwd is null", async () => {
    const result = await verifyAndBuildResumeCommand("claude", null, "tuic-uuid-1", "old-session-id");
    expect(mockRpc).not.toHaveBeenCalled();
    expect(result).toBe("claude --resume old-session-id");
  });

  it("skips verification for agents without sessionDiscovery", async () => {
    const result = await verifyAndBuildResumeCommand("aider", "/tmp/repo", "tuic-uuid-1", null);
    expect(mockRpc).not.toHaveBeenCalled();
    expect(result).toBe("aider --restore-chat-history");
  });

  it("returns null for agents without resume support", async () => {
    const result = await verifyAndBuildResumeCommand("warp", "/tmp/repo", "tuic-uuid-1", null);
    expect(result).toBeNull();
  });

  it("verifies gemini tuicSession correctly", async () => {
    mockRpc.mockResolvedValueOnce(true);
    const result = await verifyAndBuildResumeCommand("gemini", "/tmp/repo", "tuic-uuid-1", null);
    expect(mockRpc).toHaveBeenCalledWith("verify_agent_session", {
      agentType: "gemini",
      sessionId: "tuic-uuid-1",
      cwd: "/tmp/repo",
    });
    expect(result).toBe("gemini --resume tuic-uuid-1");
  });
});
