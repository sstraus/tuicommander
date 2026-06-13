/**
 * Tests for resolveHeadlessAgent logic inside useSmartPrompts.
 *
 * resolveHeadlessAgent is a private function — we exercise it through
 * canExecute() with executionMode="headless", which is the only call site.
 * We mock agentConfigsStore, providerRegistryStore, appLogger, and usePty
 * to keep tests focused on the resolution logic.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Must be declared BEFORE importing the module under test.
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: vi.fn(),
		getHeadlessTemplate: vi.fn(),
	},
}));

vi.mock("../../stores/providerRegistry", () => ({
	providerRegistryStore: { resolveSlot: vi.fn() },
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: { getActive: vi.fn(), isBusy: vi.fn() },
}));

vi.mock("../../stores/github", () => ({
	githubStore: { getPrForBranch: vi.fn() },
}));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { getActive: vi.fn(), getRevision: vi.fn(), get: vi.fn() },
}));

vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		processContent: vi.fn(),
		markAsUsed: vi.fn(),
	},
}));

vi.mock("../../utils/promptContext", () => ({
	prContextVariables: vi.fn().mockResolvedValue({}),
}));

vi.mock("../../transport", () => ({
	isTauri: false,
	rpc: {},
}));

vi.mock("../../platform", () => ({
	isWindows: () => false,
}));

const ptyMocks = vi.hoisted(() => ({
	sendCommand: vi.fn(),
	write: vi.fn(),
}));

vi.mock("../usePty", () => ({
	usePty: vi.fn(() => ({
		createSession: vi.fn(),
		closeSession: vi.fn(),
		sendInput: vi.fn(),
		sendCommand: ptyMocks.sendCommand,
		write: ptyMocks.write,
	})),
}));

import { invoke } from "../../invoke";
import { agentConfigsStore } from "../../stores/agentConfigs";
import { appLogger } from "../../stores/appLogger";
import { promptLibraryStore, type SavedPrompt } from "../../stores/promptLibrary";
import { providerRegistryStore } from "../../stores/providerRegistry";
import { terminalsStore } from "../../stores/terminals";
import { useSmartPrompts } from "../useSmartPrompts";

const mockedGetHeadlessAgent = vi.mocked(agentConfigsStore.getHeadlessAgent);
const mockedGetHeadlessTemplate = vi.mocked(agentConfigsStore.getHeadlessTemplate);
const mockedResolveSlot = vi.mocked(providerRegistryStore.resolveSlot);
const mockedWarn = vi.mocked(appLogger.warn);
const mockedGetActive = vi.mocked(terminalsStore.getActive);
const mockedIsBusy = vi.mocked(terminalsStore.isBusy);

/** Build a minimal SavedPrompt fixture with headless executionMode */
function makePrompt(overrides: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "test-prompt",
		name: "Test Prompt",
		content: "Do something",
		category: "custom",
		isFavorite: false,
		createdAt: 1_000_000,
		updatedAt: 1_000_000,
		executionMode: "headless",
		...overrides,
	};
}

/** Minimal valid resolveSlot result — only the shape matters for canExecute checks */
const CONFIGURED_SLOT = {
	provider: { id: "p1", name: "Test", type: "openai" },
	model: { id: "m1", name: "gpt-4o" },
} as unknown as ReturnType<typeof providerRegistryStore.resolveSlot>;

beforeEach(() => {
	vi.clearAllMocks();
	// Default: headless provider is configured
	mockedResolveSlot.mockReturnValue(CONFIGURED_SLOT);
});

afterEach(() => {
	vi.clearAllMocks();
});

describe("resolveHeadlessAgent — preferred='api'", () => {
	it("returns isApi=true when preferred is 'api'", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		// With isApi=true and a configured headless provider, canExecute returns ok
		const result = canExecute(makePrompt({ preferredAgent: "api" }));
		expect(result.ok).toBe(true);
	});

	it("returns ok=false when preferred='api' but no headless provider configured", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		mockedResolveSlot.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "api" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/Headless provider not configured/);
	});
});

describe("resolveHeadlessAgent — preferred agent with template", () => {
	it("returns the preferred agent when it has a template", () => {
		mockedGetHeadlessTemplate.mockReturnValue("claude --headless {prompt}");
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(result.ok).toBe(true);
		// getHeadlessTemplate must have been called with the preferred agent
		expect(mockedGetHeadlessTemplate).toHaveBeenCalledWith("claude");
	});
});

describe("resolveHeadlessAgent — preferred agent with no template", () => {
	it("falls back to global when preferred has no template", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue("gemini");
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		// Falls back to global (gemini) which is non-null → ok=true
		expect(result.ok).toBe(true);
	});

	it("logs a warning when falling back to global", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue("gemini");
		const { canExecute } = useSmartPrompts();
		canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(mockedWarn).toHaveBeenCalledWith("prompts", expect.stringContaining("claude"));
	});

	it("returns ok=false when preferred has no template and global is null", () => {
		mockedGetHeadlessTemplate.mockReturnValue(undefined);
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: "claude" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/No headless agent configured/);
	});
});

describe("resolveHeadlessAgent — no preferred agent", () => {
	it("returns ok=false when no preferred and global is null", () => {
		mockedGetHeadlessAgent.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/No headless agent configured/);
	});

	it("returns ok=true when no preferred and global is a valid agent", () => {
		mockedGetHeadlessAgent.mockReturnValue("claude");
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(true);
	});

	it("returns isApi=true when no preferred and global='api'", () => {
		mockedGetHeadlessAgent.mockReturnValue("api");
		const { canExecute } = useSmartPrompts();
		// isApi=true triggers the provider check
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(true);
		expect(mockedResolveSlot).toHaveBeenCalledWith("headless");
	});

	it("returns ok=false when no preferred, global='api', but no headless provider", () => {
		mockedGetHeadlessAgent.mockReturnValue("api");
		mockedResolveSlot.mockReturnValue(null);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ preferredAgent: undefined }));
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/Headless provider not configured/);
	});
});

describe("canExecuteInject — idle gate by inject target", () => {
	const ACTIVE = { id: "t1", sessionId: "s1", agentType: "claude" } as unknown as ReturnType<
		typeof terminalsStore.getActive
	>;

	beforeEach(() => {
		mockedGetActive.mockReturnValue(ACTIVE);
	});

	it("compose target (default) is not gated by a busy agent", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		// injectTarget unset → defaults to "compose"
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: undefined }));
		expect(result.ok).toBe(true);
		// Idle was never consulted for compose
		expect(mockedIsBusy).not.toHaveBeenCalled();
	});

	it("terminal target is blocked while the agent is busy", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("Agent is busy");
	});

	it("terminal target is allowed when the agent is idle", () => {
		mockedIsBusy.mockReturnValue(false);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));
		expect(result.ok).toBe(true);
	});

	it("terminal target with requiresIdle=false is allowed even while busy", () => {
		mockedIsBusy.mockReturnValue(true);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "terminal", requiresIdle: false }));
		expect(result.ok).toBe(true);
	});

	it("requires an active terminal with a detected agent regardless of target", () => {
		mockedGetActive.mockReturnValue(undefined);
		const { canExecute } = useSmartPrompts();
		const result = canExecute(makePrompt({ executionMode: "inject", injectTarget: "compose" }));
		expect(result.ok).toBe(false);
		expect(result.reason).toBe("No active terminal");
	});
});

describe("executeInject — routing by inject target", () => {
	const mockedInvoke = vi.mocked(invoke);
	const mockedProcess = vi.mocked(promptLibraryStore.processContent);
	const PROCESSED = "PROCESSED CONTENT";

	/** Active terminal with an optional compose-box ref. */
	const activeWith = (openComposeWithText?: (t: string) => void) =>
		({
			id: "t1",
			sessionId: "s1",
			agentType: "claude",
			ref: openComposeWithText ? { openComposeWithText } : undefined,
		}) as unknown as ReturnType<typeof terminalsStore.getActive>;

	beforeEach(() => {
		mockedIsBusy.mockReturnValue(false);
		// resolve_prompt_variables → no variables needed.
		mockedInvoke.mockResolvedValue({ vars: {}, needed: [] });
		mockedProcess.mockResolvedValue(PROCESSED);
	});

	it("compose target fills the compose box and never touches the PTY", async () => {
		const openCompose = vi.fn();
		mockedGetActive.mockReturnValue(activeWith(openCompose));
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "compose" }));

		expect(res.ok).toBe(true);
		expect(openCompose).toHaveBeenCalledWith(PROCESSED);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("terminal target sends straight to the agent via sendCommand", async () => {
		mockedGetActive.mockReturnValue(activeWith());
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "terminal" }));

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("s1", PROCESSED, "claude");
		expect(ptyMocks.write).not.toHaveBeenCalled();
	});

	it("terminal target with autoExecute=false writes for review (no Enter), not sendCommand", async () => {
		mockedGetActive.mockReturnValue(activeWith());
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(
			makePrompt({ executionMode: "inject", injectTarget: "terminal", autoExecute: false }),
		);

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		// \x15 (NAK) clears the line before writing the reviewable text.
		expect(ptyMocks.write).toHaveBeenCalledWith("s1", `\x15${PROCESSED}`);
	});

	it("compose target with no compose panel (web/PWA) falls back to a reviewable write", async () => {
		mockedGetActive.mockReturnValue(activeWith()); // no openComposeWithText
		const { executeSmartPrompt } = useSmartPrompts();

		const res = await executeSmartPrompt(makePrompt({ executionMode: "inject", injectTarget: "compose" }));

		expect(res.ok).toBe(true);
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
		expect(ptyMocks.write).toHaveBeenCalledWith("s1", `\x15${PROCESSED}`);
	});
});
