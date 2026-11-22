import { beforeEach, describe, expect, it, vi } from "vitest";
import { AGENTS } from "../../agents";
import {
	buildAgentLaunchCommand,
	prepareAgentLaunchCommand,
	verifyAndBuildResumeCommand,
} from "../../utils/agentSession";

const { mockAgentConfigsStore } = vi.hoisted(() => ({
	mockAgentConfigsStore: {
		getDefaultConfig: vi.fn().mockReturnValue(undefined),
		getRunConfigs: vi.fn().mockReturnValue([]),
	},
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: mockAgentConfigsStore,
	// Every machine answers with the same stub here: which machine is chosen is
	// asserted in remoteRepoRouting.test.ts, against the URL that leaves the
	// process. These cases are about the command string the run config produces.
	agentConfigsForRepo: () => mockAgentConfigsStore,
	ensureAgentConfigsForRepo: () => Promise.resolve(mockAgentConfigsStore),
}));

// Mock rpc for verifyAndBuildResumeCommand tests
const mockRpc = vi.fn();
vi.mock("../../transport", () => ({
	rpc: (...args: unknown[]) => mockRpc(...args),
}));

describe("buildAgentLaunchCommand", () => {
	it("leaves screen policy to the Rust launch-args builder", () => {
		expect(buildAgentLaunchCommand("codex", null, "codex")).toBe("codex");
		expect(buildAgentLaunchCommand("opencode run", null, "opencode")).toBe("opencode run");
	});
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
		expect(buildAgentLaunchCommand("agy", "abc-123")).toBe("agy");
	});

	it("injects --session-id into full command with args", () => {
		expect(buildAgentLaunchCommand("claude --model opus", "abc-123")).toBe("claude --session-id abc-123 --model opus");
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

describe("prepareAgentLaunchCommand", () => {
	beforeEach(() => mockRpc.mockReset());

	it("uses Rust's decision for a supported interactive launch", async () => {
		mockRpc.mockResolvedValueOnce(["--no-alt-screen", "resume"]);
		expect(await prepareAgentLaunchCommand("codex resume", null, "codex")).toBe("codex --no-alt-screen resume");
		expect(mockRpc).toHaveBeenCalledWith("prepare_agent_launch_args", {
			agentType: "codex",
			binaryPath: "codex",
			args: ["resume"],
			repoPath: undefined,
		});
	});

	it("preserves non-interactive subcommands and excludes other agents", async () => {
		mockRpc.mockResolvedValueOnce(["run"]).mockResolvedValueOnce(["exec"]).mockResolvedValueOnce([]);
		expect(await prepareAgentLaunchCommand("opencode run", null, "opencode")).toBe("opencode run");
		expect(await prepareAgentLaunchCommand("codex exec", null, "codex")).toBe("codex exec");
		expect(await prepareAgentLaunchCommand("agy", null, "agy")).toBe("agy");
	});

	it("preserves an inline environment assignment while Rust chooses the screen flag", async () => {
		mockRpc.mockResolvedValueOnce(["--no-alt-screen"]).mockResolvedValueOnce(["--no-alt-screen"]);
		expect(await prepareAgentLaunchCommand("PROFILE=work codex", null, "codex")).toBe(
			"PROFILE=work codex --no-alt-screen",
		);
		expect(await prepareAgentLaunchCommand("PROFILE='work' codex", null, "codex")).toBe(
			"PROFILE='work' codex --no-alt-screen",
		);
		expect(mockRpc).toHaveBeenCalledWith("prepare_agent_launch_args", {
			agentType: "codex",
			binaryPath: "codex",
			args: [],
			repoPath: undefined,
		});
	});
});

describe("sessionDiscovery in AgentConfig", () => {
	it("claude has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.claude.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("claude --resume test-uuid");
	});

	it("agy has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.agy.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("agy --conversation test-uuid");
	});

	it("codex has sessionDiscovery with resumeWithId", () => {
		const disc = AGENTS.codex.sessionDiscovery;
		expect(disc).not.toBeNull();
		expect(disc?.resumeWithId("test-uuid")).toBe("codex resume test-uuid");
	});

	it("amp has null sessionDiscovery (cloud-only)", () => {
		expect(AGENTS.amp.sessionDiscovery).toBeNull();
	});

	it("opencode has null sessionDiscovery (SQLite, not implemented)", () => {
		expect(AGENTS.opencode.sessionDiscovery).toBeNull();
	});
});

describe("verifyAndBuildResumeCommand", () => {
	beforeEach(() => mockRpc.mockReset());
	// Catches: client substitution of TUIC identity for the discovered agent id.
	it("passes the exact resume context to Rust and returns its command", async () => {
		mockRpc.mockResolvedValue({ cmd: "CLAUDE_CONFIG_DIR=private claude --resume exact", verified: true });
		expect(await verifyAndBuildResumeCommand("claude", "/repo", "peer", "exact", "private claude")).toBe(
			"CLAUDE_CONFIG_DIR=private claude --resume exact",
		);
		expect(mockRpc).toHaveBeenCalledWith("suspended_session_action", {
			cwd: "/repo",
			request: {
				action: "resolve_resume",
				agentType: "claude",
				cwd: "/repo",
				tuicSession: "peer",
				agentSessionId: "exact",
				launchCommand: "private claude",
			},
		});
	});
	// Catches: frontend rebuilding a command for an agent Rust could not verify.
	it("keeps the Rust refusal instead of choosing a local fallback", async () => {
		mockRpc.mockResolvedValue({ cmd: null, verified: false });
		expect(await verifyAndBuildResumeCommand("claude", "/repo", "peer", "gone")).toBeNull();
	});
});
