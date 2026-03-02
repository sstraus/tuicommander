import { describe, it, expect } from "vitest";
import { buildAgentLaunchCommand, buildResumeCommand } from "../../utils/agentSession";
import { AGENTS } from "../../agents";

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

  it("returns static resume for gemini regardless of UUID", () => {
    expect(buildResumeCommand("gemini", null)).toBe("gemini --resume");
  });

  it("returns static resume for aider", () => {
    expect(buildResumeCommand("aider", null)).toBe("aider --restore-chat-history");
  });

  it("returns static resume for codex", () => {
    expect(buildResumeCommand("codex", null)).toBe("codex resume --last");
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
