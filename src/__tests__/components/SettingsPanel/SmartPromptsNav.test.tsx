import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import type { SavedPrompt } from "../../../stores/promptLibrary";
import { nextAnimationFrame } from "../../helpers/animationFrame";

/** One smart prompt, enough to exercise the row → editor path */
const SMART_PROMPT: SavedPrompt = {
	id: "sp-test",
	name: "Test Smart Prompt",
	content: "Do the thing",
	category: "custom",
	isFavorite: false,
	tags: ["smart", "git"],
	enabled: true,
	placement: ["toolbar"],
	executionMode: "inject",
	injectTarget: "terminal",
	autoExecute: false,
	createdAt: 0,
	updatedAt: 0,
};

const updatePrompt = vi.fn();

vi.mock("../../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getAllPrompts: () => [SMART_PROMPT],
		updatePrompt: (...args: unknown[]) => updatePrompt(...args),
		createPrompt: vi.fn(),
		deletePrompt: vi.fn(),
		resetToDefault: vi.fn(),
		isOverridden: () => false,
		hasUpdate: () => false,
	},
}));

vi.mock("../../../data/smartPromptsBuiltIn", () => ({ SMART_PROMPTS_BUILTIN: [] }));

vi.mock("../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({
		detectAll: vi.fn(),
		getAvailable: () => [],
		loading: () => false,
	}),
}));

vi.mock("../../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: () => null,
		setHeadlessAgent: vi.fn(),
		getRunConfigs: () => [],
	},
}));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { ide: "vscode", font: "JetBrains Mono", defaultFontSize: 12, customLaunchers: [] },
		isAiChatEnabled: () => false,
		isAcpConfigured: () => false,
	},
	IDE_NAMES: { vscode: "VS Code" },
	FONT_FAMILIES: { "JetBrains Mono": "JetBrains Mono" },
}));

vi.mock("../../../stores/ui", () => ({
	uiStore: {
		state: { settingsNavWidth: 180 },
		setSettingsNavWidth: vi.fn(),
		persistUIPrefs: vi.fn(),
	},
}));

vi.mock("../../../stores/repositories", () => ({
	repositoriesStore: {
		state: { repositories: {}, repoOrder: [] },
		getAllReposOrdered: () => [],
		getConnectionId: () => undefined,
		setDisplayName: vi.fn(),
	},
}));

import { SettingsPanel } from "../../../components/SettingsPanel/SettingsPanel";

const navLabels = (container: HTMLElement) =>
	Array.from(container.querySelectorAll(".navItem")).map((n) => n.textContent);

const clickNav = (container: HTMLElement, label: string) => {
	const item = Array.from(container.querySelectorAll(".navItem")).find((n) => n.textContent === label);
	expect(item, `nav item "${label}" not found`).toBeTruthy();
	fireEvent.click(item!);
};

describe("SettingsPanel — Smart Prompts navigation", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		SMART_PROMPT.executionMode = "inject";
		SMART_PROMPT.injectTarget = "terminal";
		SMART_PROMPT.shortcut = "Cmd+Alt+G";
	});

	it("registers a Smart Prompts nav item", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		expect(navLabels(container)).toContain("Smart Prompts");
	});

	it("renders the prompt list and editor controls when the tab is selected", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} />);
		clickNav(container, "Smart Prompts");

		const heading = container.querySelector(".section h2");
		expect(heading?.textContent).toBe("Smart Prompts");
		expect(container.textContent).toContain("Test Smart Prompt");
	});

	it("opens directly on the Smart Prompts tab when requested via initialTab", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);
		const active = container.querySelector(".navItem.active");
		expect(active?.textContent).toBe("Smart Prompts");
		expect(container.querySelector(".section h2")?.textContent).toBe("Smart Prompts");
	});

	// Catches: migrating the variable trigger or native rows loses selection replacement or leaves the menu open.
	it("inserts a variable at the current content selection and closes the list", () => {
		const { getByRole, queryByRole } = render(() => (
			<SettingsPanel visible onClose={() => {}} initialTab="smart-prompts" />
		));
		fireEvent.click(getByRole("button", { name: "Edit Test Smart Prompt" }));
		const content = getByRole("textbox", { name: "Content *" }) as HTMLTextAreaElement;
		content.setSelectionRange(3, 6);
		fireEvent.click(getByRole("button", { name: "Insert variable... ▼" }));
		fireEvent.click(getByRole("button", { name: "{branch} Current branch name" }));
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", { content: "Do {branch} thing" });
		expect(queryByRole("button", { name: "{branch} Current branch name" })).toBeNull();
	});

	// Catches: the cursor-restore frame outlives the editor, so a pending timer survives test teardown.
	it("cancels the cursor-restore frame when the editor unmounts first", async () => {
		const { getByRole, unmount } = render(() => (
			<SettingsPanel visible onClose={() => {}} initialTab="smart-prompts" />
		));
		fireEvent.click(getByRole("button", { name: "Edit Test Smart Prompt" }));
		const content = getByRole("textbox", { name: "Content *" }) as HTMLTextAreaElement;
		fireEvent.click(getByRole("button", { name: "Insert variable... ▼" }));
		fireEvent.click(getByRole("button", { name: "{branch} Current branch name" }));
		const focus = vi.spyOn(content, "focus");
		unmount();
		await nextAnimationFrame();
		expect(focus).not.toHaveBeenCalled();
	});

	it("keeps execution and send-immediately controls editable in the expanded editor", () => {
		const { container } = render(() => <SettingsPanel visible={true} onClose={() => {}} initialTab="smart-prompts" />);

		// Expand the prompt row to reveal the editor
		const header = Array.from(container.querySelectorAll("button[aria-expanded]")).find((el) =>
			el.getAttribute("aria-label")?.includes("Test Smart Prompt"),
		);
		expect(header).toBeTruthy();
		fireEvent.click(header!);

		const labels = Array.from(container.querySelectorAll("label")).map((l) => l.textContent);
		expect(labels).toContain("Execution");
		expect(labels).toContain("Send immediately");

		// Execution Mode is editable
		const modeSelect = Array.from(container.querySelectorAll("select")).find((sel) =>
			Array.from(sel.options).some((o) => o.value === "headless"),
		);
		expect(modeSelect).toBeTruthy();
		fireEvent.change(modeSelect!, { target: { value: "headless" } });
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", expect.objectContaining({ executionMode: "headless" }));

		// Auto-execute is editable
		updatePrompt.mockClear();
		const checkbox = container.querySelector('input[role="switch"][id]') as HTMLInputElement | null;
		expect(checkbox).toBeTruthy();
		fireEvent.change(checkbox!, { target: { checked: true } });
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", expect.objectContaining({ autoExecute: true }));
	});
	// Catches: readable placement labels saving display text instead of the stable placement id.
	it("keeps readable placement choices bound to stored ids and clears the current shortcut", () => {
		const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="smart-prompts" />);
		fireEvent.click(view.getByRole("button", { name: "Edit Test Smart Prompt" }));
		fireEvent.click(view.getByRole("checkbox", { name: "File menu" }));
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", { placement: ["toolbar", "file-context"] });
		expect(view.getByRole("button", { name: "Shortcut" }).textContent).toBe("Cmd+Alt+G");
		expect(view.getByRole("button", { name: "Shortcut" }).id).toBe(
			view.getByText("Shortcut", { selector: "label" }).getAttribute("for"),
		);
		fireEvent.click(view.getByRole("button", { name: "Clear shortcut" }));
		expect(updatePrompt).toHaveBeenCalledWith("sp-test", { shortcut: "" });
	});

	// Catches: flattening the form makes terminal-only or mode-specific fields unconditional.
	it.each([
		["inject", "compose", ["Target"]],
		["inject", "terminal", ["Target", "Send immediately"]],
		["shell", "terminal", ["Output Target"]],
		["headless", "terminal", ["Preferred Agent", "Output Target"]],
		["api", "terminal", ["Output Target", "System Prompt"]],
	] as const)("preserves conditional fields for %s / %s", (mode, target, expected) => {
		SMART_PROMPT.executionMode = mode;
		SMART_PROMPT.injectTarget = target;
		const view = render(() => <SettingsPanel visible onClose={() => {}} initialTab="smart-prompts" />);
		fireEvent.click(view.getByRole("button", { name: "Edit Test Smart Prompt" }));
		const labels = [...view.container.querySelectorAll("label[for]")].map((l) => l.textContent);
		for (const label of ["Target", "Send immediately", "Preferred Agent", "Output Target", "System Prompt"]) {
			expect(labels.includes(label), label).toBe((expected as readonly string[]).includes(label));
		}
		expect(view.container.textContent).not.toContain("Uncheck to review");
	});
});
