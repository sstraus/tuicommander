import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ptyMocks = vi.hoisted(() => ({
	sendCommand: vi.fn().mockResolvedValue(undefined),
}));

const terminalMocks = vi.hoisted(() => ({
	openComposeWithText: vi.fn(),
}));

const mockInvoke = vi.hoisted(() => vi.fn());

vi.mock("../../invoke", () => ({ invoke: mockInvoke }));

vi.mock("../../hooks/usePty", () => ({
	usePty: () => ({
		sendCommand: ptyMocks.sendCommand,
	}),
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		getActive: () => ({
			id: "terminal-1",
			sessionId: "session-1",
			agentType: "codex",
			ref: { openComposeWithText: terminalMocks.openComposeWithText },
		}),
	},
}));

import { PromptDrawer } from "../../components/PromptDrawer/PromptDrawer";
import { promptLibraryStore } from "../../stores/promptLibrary";

function buttonByText(container: HTMLElement, text: string): HTMLButtonElement {
	const button = Array.from(container.querySelectorAll("button")).find(
		(candidate) => candidate.textContent?.trim() === text,
	);
	expect(button, `button "${text}" not found`).toBeTruthy();
	return button as HTMLButtonElement;
}

function createPromptThroughEditor(
	container: HTMLElement,
	name: string,
	content: string,
	autoExecute: boolean,
): HTMLElement {
	fireEvent.click(buttonByText(container, "+ New Prompt"));

	const nameInput = container.querySelector('input[placeholder="My Prompt"]') as HTMLInputElement;
	const contentInput = container.querySelector(
		'textarea[placeholder="Enter your prompt text here..."]',
	) as HTMLTextAreaElement;
	expect(nameInput).toBeTruthy();
	expect(contentInput).toBeTruthy();
	fireEvent.input(nameInput, { target: { value: name } });
	fireEvent.input(contentInput, { target: { value: content } });

	if (autoExecute) {
		const checkbox = Array.from(container.querySelectorAll('input[type="checkbox"]')).find((candidate) =>
			candidate.closest("label")?.textContent?.includes("Send immediately"),
		) as HTMLInputElement | undefined;
		expect(checkbox).toBeTruthy();
		fireEvent.click(checkbox!);
	}

	fireEvent.click(buttonByText(container, "Save"));
	const row = Array.from(container.querySelectorAll(".promptItem")).find((candidate) =>
		candidate.textContent?.includes(name),
	);
	expect(row).toBeTruthy();
	return row as HTMLElement;
}

describe("PromptDrawer auto-execute", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		mockInvoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
			if (command === "extract_prompt_variables") {
				const content = String(args?.content ?? "");
				return Array.from(content.matchAll(/\{([^{}]+)\}/g), (match) => match[1]);
			}
			if (command === "process_prompt_content") {
				const variables = (args?.variables ?? {}) as Record<string, string>;
				return String(args?.content ?? "").replace(/\{([^{}]+)\}/g, (_match, name: string) => variables[name] ?? "");
			}
			return undefined;
		});
		for (const prompt of promptLibraryStore.getAllPrompts()) {
			promptLibraryStore.deletePrompt(prompt.id);
		}
		promptLibraryStore._testCancelPendingSave();
		promptLibraryStore.setSelectedCategory("all");
		promptLibraryStore.openDrawer();
	});

	afterEach(() => {
		promptLibraryStore.closeDrawer();
		promptLibraryStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	// Catches: category actions losing their selected state or closing the drawer on click.
	it("keeps category selection and Escape dismissal with shared buttons", () => {
		const view = render(() => <PromptDrawer />);
		const favorites = buttonByText(view.container, "Favorites");
		fireEvent.click(favorites);
		expect(favorites.getAttribute("aria-pressed")).toBe("true");
		expect(buttonByText(view.container, "All").getAttribute("aria-pressed")).toBe("false");
		expect(promptLibraryStore.state.drawerOpen).toBe(true);
		fireEvent.keyDown(document, { key: "Escape" });
		expect(promptLibraryStore.state.drawerOpen).toBe(false);
	});

	// Catches: execution-mode controls reintroduce always-visible helper captions.
	it("renders prompt controls without invented guidance", () => {
		const view = render(() => <PromptDrawer />);
		fireEvent.click(buttonByText(view.container, "+ New Prompt"));
		expect(view.container.textContent).toContain("Send immediately");
		expect(view.container.textContent).not.toContain("Uncheck to review before sending");
		const mode = Array.from(view.container.querySelectorAll("select")).find((select) =>
			Array.from(select.options).some((option) => option.value === "api"),
		)!;
		fireEvent.change(mode, { target: { value: "api" } });
		expect(view.container.textContent).toContain("System Prompt");
		expect(view.container.textContent).not.toContain("Sent as the system message to the LLM");
	});

	it("submits an editor-created prompt with autoExecute enabled exactly once", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Run custom prompt", "Do the custom task", true);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Do the custom task", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("keeps an editor-created prompt editable when autoExecute is disabled", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Review custom prompt", "Review before sending", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);

		expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce();
		expect(terminalMocks.openComposeWithText).toHaveBeenCalledWith("Review before sending");
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});

	it("turns a double-click into one explicit submission without also inserting", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Double-click prompt", "Run on double-click", false);

		fireEvent.click(row, { detail: 1 });
		fireEvent.click(row, { detail: 2 });
		fireEvent.dblClick(row, { detail: 2 });
		await vi.advanceTimersByTimeAsync(250);

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Run on double-click", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("injects a slow double-click only once", async () => {
		// The 200ms de-dup timer is shorter than macOS's configurable double-click
		// interval (up to ~1s). When the timer wins the race the single-click
		// injection has already run, and the dblclick that follows must not inject
		// the prompt a second time — least of all submitting it.
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Slow double-click", "Do it once", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(400); // the pending click fires first
		fireEvent.click(row, { detail: 2 });
		fireEvent.dblClick(row, { detail: 2 });
		await vi.advanceTimersByTimeAsync(250);

		// What actually holds the line is `doInject` closing the drawer: the row
		// unmounts, so the trailing dblclick reaches no handler at all. That is load
		// bearing, not incidental — this test fails the moment injection stops
		// closing the drawer.
		expect(document.body.contains(row)).toBe(false);
		const injections = ptyMocks.sendCommand.mock.calls.length + terminalMocks.openComposeWithText.mock.calls.length;
		expect(injections).toBe(1);
	});

	it("lets Insert and Run override a disabled autoExecute flag after variable entry", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Variable prompt", "Handle {topic}", false);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		const variableInput = container.querySelector('input[placeholder="topic"]') as HTMLInputElement;
		expect(variableInput).toBeTruthy();
		fireEvent.input(variableInput, { target: { value: "tests" } });
		fireEvent.click(buttonByText(container, "Insert & Run"));
		await vi.waitFor(() => expect(ptyMocks.sendCommand).toHaveBeenCalledOnce());

		expect(ptyMocks.sendCommand).toHaveBeenCalledOnce();
		expect(ptyMocks.sendCommand).toHaveBeenCalledWith("session-1", "Handle tests", "codex", true);
		expect(terminalMocks.openComposeWithText).not.toHaveBeenCalled();
	});

	it("lets Insert override an enabled autoExecute flag after variable entry", async () => {
		const { container } = render(() => <PromptDrawer />);
		const row = createPromptThroughEditor(container, "Editable variable prompt", "Handle {topic}", true);

		fireEvent.click(row, { detail: 1 });
		await vi.advanceTimersByTimeAsync(250);
		const variableInput = container.querySelector('input[placeholder="topic"]') as HTMLInputElement;
		fireEvent.input(variableInput, { target: { value: "tests" } });
		fireEvent.click(buttonByText(container, "Insert"));
		await vi.waitFor(() => expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce());

		expect(terminalMocks.openComposeWithText).toHaveBeenCalledOnce();
		expect(terminalMocks.openComposeWithText).toHaveBeenCalledWith("Handle tests");
		expect(ptyMocks.sendCommand).not.toHaveBeenCalled();
	});
});
