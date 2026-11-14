import { createRoot, createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
	branch: "feature/topic",
	base: "main",
	cwd: "/repo-feature",
	invoke: vi.fn(),
	sendCommand: vi.fn(),
}));
vi.mock("../../invoke", () => ({ invoke: state.invoke }));
vi.mock("../../stores/settings", () => ({ settingsStore: { isAcpConfigured: () => true } }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({ id: "t1", sessionId: "s1", agentType: "ego", cwd: state.cwd }),
		isBusy: () => false,
	},
}));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		getActive: () => ({
			path: "/repo",
			activeWorkspaceId: "workspace-uuid",
			workspaces: {
				"workspace-uuid": { branchName: state.branch, worktreePath: state.cwd },
			},
		}),
		get: () => undefined,
		getGitRevision: () => revision(),
	},
}));
vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: { getEffectiveField: () => state.base },
}));
vi.mock("../../stores/github", () => ({ githubStore: { getBranchPrData: () => null } }));
vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		markAsUsed: vi.fn(),
		processContent: async (prompt: { content: string }, vars: Record<string, string>) =>
			prompt.content.replace(/\{(\w+)\}/g, (_, name: string) => vars[name] ?? `{${name}}`),
	},
}));
vi.mock("../../stores/agentConfigs", () => ({ agentConfigsStore: {} }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("../../hooks/usePty", () => ({ usePty: () => ({ sendCommand: state.sendCommand }) }));

import { SMART_PROMPTS_BUILTIN } from "../../data/smartPromptsBuiltIn";
import { useSmartPrompts } from "../../hooks/useSmartPrompts";
import type { SavedPrompt } from "../../stores/promptLibrary";

const prompt: SavedPrompt = {
	id: "branch-merge",
	name: "Merge",
	content: "Merge {branch} into {base_branch}",
	category: "custom",
	isFavorite: false,
	createdAt: 1,
	updatedAt: 1,
	executionMode: "inject",
	injectTarget: "terminal",
	autoExecute: true,
};

const [revision, setRevision] = createSignal(0);
const disposers: (() => void)[] = [];
function mountedHook() {
	return createRoot((dispose) => {
		disposers.push(dispose);
		return useSmartPrompts();
	});
}
afterEach(() => {
	for (const dispose of disposers.splice(0)) dispose();
});

beforeEach(() => {
	vi.clearAllMocks();
	setRevision(0);
	state.branch = "feature/topic";
	state.base = "main";
	state.cwd = "/repo-feature";
	// Adapter contract: Git resolves HEAD in the directory passed by its caller.
	state.invoke.mockImplementation(async (_cmd: string, args: { repoPath: string }) => ({
		vars: { branch: args.repoPath === "/repo" ? "main" : state.branch, base_branch: "main" },
		needed: ["branch", "base_branch"],
	}));
});

describe("branch prompt execution", () => {
	it.each(["main", "master", "develop"])(
		"Automatic keeps detected %s instead of injecting the sentinel (F1)",
		async (base) => {
			state.base = "automatic";
			state.invoke.mockResolvedValue({
				vars: { branch: "feature/topic", base_branch: base },
				needed: ["branch", "base_branch"],
			});
			expect(await useSmartPrompts().executeSmartPrompt(prompt)).toEqual({ ok: true });
			expect(state.sendCommand).toHaveBeenCalledWith("s1", `Merge feature/topic into ${base}`, "ego", true);
		},
	);

	it("Automatic refuses a self-merge against the detected base (F1)", async () => {
		state.base = "automatic";
		state.invoke.mockResolvedValue({
			vars: { branch: "main", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		expect(await useSmartPrompts().executeSmartPrompt(prompt)).toEqual({
			ok: false,
			reason: expect.stringContaining("base branch"),
		});
		expect(state.sendCommand).not.toHaveBeenCalled();
	});

	it("cached main cannot veto a real feature checkout (F2)", async () => {
		state.branch = "main";
		state.invoke.mockResolvedValue({
			vars: { branch: "feature/topic", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		const hook = mountedHook();
		await vi.waitFor(() =>
			expect(state.invoke).toHaveBeenCalledWith("resolve_prompt_variables", {
				content: "{branch} {base_branch}",
				repoPath: "/repo-feature",
			}),
		);
		expect(hook.canExecute(prompt)).toEqual({ ok: true });
		expect(await hook.executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into main", "ego", true);
	});

	it("direct execution resolves a feature checkout even when cached main would refuse (F2)", async () => {
		state.branch = "main";
		state.invoke.mockResolvedValue({
			vars: { branch: "feature/topic", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		expect(await useSmartPrompts().executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into main", "ego", true);
	});

	it("selected checkout resolves feature and configured base before injection", async () => {
		state.base = "develop";
		expect(await useSmartPrompts().executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.invoke).toHaveBeenCalledWith("resolve_prompt_variables", {
			content: prompt.content,
			repoPath: "/repo-feature",
		});
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into develop", "ego", true);
	});

	it("configured base resolves the built-in merge when Git has no detectable base", async () => {
		// Catches refusing explicit settings in feature-only or local-only repositories.
		state.base = "release/stable";
		state.invoke.mockResolvedValue({ vars: { branch: "feature/topic" }, needed: ["branch", "base_branch"] });
		const merge = SMART_PROMPTS_BUILTIN.find((p) => p.id === "smart-merge-main")!;
		expect(await useSmartPrompts().executeSmartPrompt(merge)).toEqual({ ok: true });
		expect(state.sendCommand.mock.calls[0][1]).toContain("Merge the latest release/stable into feature/topic.");
	});

	it("self-merge custom and built-in prompts are unavailable and inject nothing", async () => {
		state.branch = "main";
		state.cwd = "/repo";
		for (const item of [
			prompt,
			...SMART_PROMPTS_BUILTIN.filter((p) => ["smart-merge-main", "smart-rebase-main"].includes(p.id)),
		]) {
			const hook = mountedHook();
			await vi.waitFor(() =>
				expect(hook.canExecute(item)).toEqual({ ok: false, reason: expect.stringContaining("base branch") }),
			);
			expect(await hook.executeSmartPrompt(item)).toEqual({
				ok: false,
				reason: expect.stringContaining("base branch"),
			});
		}
		expect(state.sendCommand).not.toHaveBeenCalled();
	});

	it("fresh Git state prevents self-merge when workspace metadata is stale", async () => {
		state.invoke.mockResolvedValue({
			vars: { branch: "main", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		expect((await useSmartPrompts().executeSmartPrompt(prompt)).ok).toBe(false);
		expect(state.sendCommand).not.toHaveBeenCalled();
	});

	it.each(["main", "master", "develop"])(
		"Automatic availability refuses detected %s without cached workspace equality",
		async (base) => {
			state.base = "automatic";
			state.invoke.mockResolvedValue({ vars: { branch: base, base_branch: base }, needed: ["branch", "base_branch"] });
			const hook = mountedHook();
			await vi.waitFor(() =>
				expect(hook.canExecute(prompt)).toEqual({
					ok: false,
					reason: expect.stringContaining(`base branch "${base}"`),
				}),
			);
		},
	);

	it("Git revision refresh removes a stale refusal when the checkout becomes a feature branch (F2)", async () => {
		state.branch = "main";
		state.invoke.mockResolvedValue({
			vars: { branch: "main", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		const hook = mountedHook();
		await vi.waitFor(() => expect(hook.canExecute(prompt).ok).toBe(false));
		state.invoke.mockResolvedValue({
			vars: { branch: "feature/topic", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		setRevision(1);
		await vi.waitFor(() => expect(hook.canExecute(prompt).ok).toBe(true));
		expect(await hook.executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into main", "ego", true);
	});

	it("a changed target checkout does not retain another terminal's base refusal", async () => {
		state.cwd = "/repo";
		const hook = mountedHook();
		await vi.waitFor(() => expect(hook.canExecute(prompt).ok).toBe(false));
		state.cwd = "/repo-feature";
		setRevision(1);
		await vi.waitFor(() =>
			expect(state.invoke).toHaveBeenCalledWith("resolve_prompt_variables", {
				content: "{branch} {base_branch}",
				repoPath: "/repo-feature",
			}),
		);
		expect(hook.canExecute(prompt).ok).toBe(true);
		expect(await hook.executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into main", "ego", true);
	});

	it("execution rechecks Git even when mounted availability still refuses", async () => {
		state.invoke.mockResolvedValue({
			vars: { branch: "main", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		const hook = mountedHook();
		await vi.waitFor(() => expect(hook.canExecute(prompt).ok).toBe(false));
		state.invoke.mockResolvedValue({
			vars: { branch: "feature/topic", base_branch: "main" },
			needed: ["branch", "base_branch"],
		});
		expect(await hook.executeSmartPrompt(prompt)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Merge feature/topic into main", "ego", true);
	});

	it("ordinary single-branch prompts remain available on the base branch", async () => {
		state.branch = "main";
		state.cwd = "/repo";
		const ordinary = { ...prompt, content: "Review {branch}" };
		state.invoke.mockResolvedValue({ vars: { branch: "main" }, needed: ["branch"] });
		expect(await useSmartPrompts().executeSmartPrompt(ordinary)).toEqual({ ok: true });
		expect(state.sendCommand).toHaveBeenCalledWith("s1", "Review main", "ego", true);
	});
});
