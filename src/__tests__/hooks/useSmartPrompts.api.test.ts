/**
 * api mode is one unattended ego turn (#787-ee50).
 *
 * TUICommander holds no provider registry and makes no provider call any more,
 * so everything a case here can exercise is the shape around the turn: what is
 * refused before ego is launched, what reaches the command, and what a turn
 * that refused a question is reported as. The turn itself is folded in Rust and
 * tested there.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, settings, terminals, repositories, markAsUsed, writeClipboard, warn, toast } = vi.hoisted(() => ({
	invoke: vi.fn(),
	settings: { configured: true },
	terminals: { active: null as { id: string; cwd?: string; agentType?: string } | null },
	repositories: { active: null as { path: string } | null },
	markAsUsed: vi.fn(),
	writeClipboard: vi.fn(async () => {}),
	warn: vi.fn(),
	toast: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toast } }));

vi.mock("../../stores/settings", () => ({
	settingsStore: { isAcpConfigured: () => settings.configured },
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: { getActive: () => terminals.active, isBusy: () => false },
}));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { getActive: () => repositories.active, get: () => undefined },
}));

vi.mock("../../stores/github", () => ({
	githubStore: { getBranchPrData: () => null },
}));

vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		markAsUsed,
		processContent: async (prompt: { content: string }) => prompt.content,
	},
}));

vi.mock("../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: () => null,
		getHeadlessTemplate: () => undefined,
		getRunConfigs: () => [],
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn, error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../utils/clipboard", () => ({ writeClipboard }));

vi.mock("../../hooks/usePty", () => ({ usePty: () => ({ sendCommand: vi.fn() }) }));

import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import type { SavedPrompt } from "../../stores/promptLibrary";
import { runWithFeedback } from "../../utils/promptContext";

/** A saved prompt with only the fields a case here cares about. */
function prompt(over: Partial<SavedPrompt> = {}): SavedPrompt {
	return {
		id: "p1",
		name: "Summarise",
		content: "summarise this",
		category: "custom",
		isFavorite: false,
		createdAt: 1,
		updatedAt: 1,
		executionMode: "api",
		...over,
	} as SavedPrompt;
}

/** ego answered `text`, having refused `declined` questions on the way. */
function turn(text: string, declined = 0) {
	return { text, stopReason: "end_turn", declined };
}

beforeEach(() => {
	vi.clearAllMocks();
	settings.configured = true;
	terminals.active = { id: "t1", cwd: "/repo" };
	repositories.active = { path: "/repo" };
	// Every execute resolves its variables first; nothing here uses one.
	invoke.mockImplementation(async (command: string) => {
		if (command === "resolve_prompt_variables") return { vars: {}, needed: [] };
		return turn("the answer");
	});
});

describe("Smart Prompts api mode", () => {
	// Catches: a structured ACP rejection reaches the toast as [object Object].
	it.each([
		[
			{ code: "agent_error", message: "Internal error: write-scoped probe failed", retryable: false },
			"agent_error: Internal error: write-scoped probe failed",
		],
		[{ message: "The agent could not start" }, "The agent could not start"],
		["agent unavailable", "agent unavailable"],
		[new Error("turn timed out"), "Error: turn timed out"],
	])("structured_prompt_failure_reaches_toast: %s", async (failure, expected) => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "resolve_prompt_variables") return { vars: {}, needed: [] };
			throw failure;
		});
		const saved = prompt({ name: "Merge in main" });
		await runWithFeedback(saved, () => useSmartPrompts().executeSmartPrompt(saved));
		expect(toast).toHaveBeenCalledWith("Merge in main", expected, "warn");
		expect(markAsUsed).not.toHaveBeenCalled();
		expect(writeClipboard).not.toHaveBeenCalled();
	});

	it("refuses before launching anything when no ego binary is named", () => {
		settings.configured = false;

		const check = useSmartPrompts().canExecute(prompt());

		expect(check.ok).toBe(false);
		// The mode is not what failed, so the reason names where each fix is: the
		// binary is on General, the model on the AI Chat page.
		expect(check.reason).toMatch(/binary in Settings → General/);
		expect(check.reason).toMatch(/model in Settings → AI Chat/);
		// The deleted provider registry page sends the user nowhere.
		expect(check.reason).not.toMatch(/Settings → AI Providers/);
	});

	it("refuses when there is no directory for the turn to run in", () => {
		terminals.active = null;
		repositories.active = null;

		const check = useSmartPrompts().canExecute(prompt());

		expect(check.ok).toBe(false);
		// An empty path would reach ego as a session it refuses to open, which
		// reads as an ego fault rather than as "open a repository first".
		expect(check.reason).toMatch(/working directory/);
	});

	it("runs the turn in the active terminal's directory", async () => {
		terminals.active = { id: "t1", cwd: "/repo/worktree" };

		await useSmartPrompts().executeSmartPrompt(prompt());

		expect(invoke).toHaveBeenCalledWith("acp_one_shot_prompt", {
			root: "/repo/worktree",
			prompt: "summarise this",
		});
	});

	it("falls back to the active repository when no terminal is open", async () => {
		terminals.active = null;
		repositories.active = { path: "/repo" };

		await useSmartPrompts().executeSmartPrompt(prompt());

		expect(invoke).toHaveBeenCalledWith("acp_one_shot_prompt", { root: "/repo", prompt: "summarise this" });
	});

	it("sends what ego said to the prompt's output target", async () => {
		const result = await useSmartPrompts().executeSmartPrompt(prompt({ outputTarget: "clipboard" }));

		expect(result).toEqual({ ok: true, output: "the answer" });
		expect(writeClipboard).toHaveBeenCalledWith("the answer");
		expect(markAsUsed).toHaveBeenCalledWith("p1");
	});

	it("does not report a refused question as an empty answer", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "resolve_prompt_variables") return { vars: {}, needed: [] };
			return turn("", 2);
		});

		const result = await useSmartPrompts().executeSmartPrompt(prompt({ outputTarget: "clipboard" }));

		// "ego returned nothing" would send the reader to the prompt. The model
		// reached for a tool an unattended turn cannot grant.
		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/2 permissions this mode cannot grant/);
		expect(writeClipboard).not.toHaveBeenCalled();
	});

	it("keeps an empty answer an empty answer when nothing was refused", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "resolve_prompt_variables") return { vars: {}, needed: [] };
			return turn("", 0);
		});

		const result = await useSmartPrompts().executeSmartPrompt(prompt());

		expect(result).toEqual({ ok: true, output: "" });
	});

	it("reports what the command failed with rather than swallowing it", async () => {
		invoke.mockImplementation(async (command: string) => {
			if (command === "resolve_prompt_variables") return { vars: {}, needed: [] };
			throw new Error("ego did not finish the turn within 300s");
		});

		const result = await useSmartPrompts().executeSmartPrompt(prompt());

		expect(result.ok).toBe(false);
		expect(result.reason).toMatch(/did not finish the turn within 300s/);
	});

	it("runs the api headless agent on the same one path", async () => {
		// "api" is a headless agent the same way it is a mode. Two paths to one
		// behaviour would be two places to fix it.
		const saved = prompt({ executionMode: "headless", preferredAgent: "api" });

		expect(useSmartPrompts().canExecute(saved).ok).toBe(true);
		await useSmartPrompts().executeSmartPrompt(saved);

		expect(invoke).toHaveBeenCalledWith("acp_one_shot_prompt", { root: "/repo", prompt: "summarise this" });
	});

	it("refuses the api headless agent for the same reason as the mode", () => {
		settings.configured = false;

		const check = useSmartPrompts().canExecute(prompt({ executionMode: "headless", preferredAgent: "api" }));

		expect(check.ok).toBe(false);
		expect(check.reason).toMatch(/binary in Settings → General/);
		expect(check.reason).toMatch(/model in Settings → AI Chat/);
		expect(check.reason).not.toMatch(/Settings → AI Providers/);
	});
});
