import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke } = vi.hoisted(() => {
	return { mockInvoke: vi.fn().mockResolvedValue(undefined) };
});

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
	convertFileSrc: (path: string) => path,
}));

vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));

import { IdeasPanel } from "../../components/IdeasPanel/IdeasPanel";
import { appLogger } from "../../stores/appLogger";
import { ideasStore } from "../../stores/ideas";

// These component cases intentionally do not hydrate or persist the notes store.
beforeEach(() => {
	vi.spyOn(appLogger, "error").mockImplementation(() => {});
});
afterEach(() => {
	for (const args of vi.mocked(appLogger.error).mock.calls) {
		expect(args).toEqual([
			"store",
			"Refusing to persist ideas before a successful hydrate — notes.json left untouched",
		]);
	}
	vi.mocked(appLogger.error).mockRestore();
});

const noop = () => {};

function renderPanel() {
	return render(() => <IdeasPanel visible={true} repoPath={null} onClose={noop} onSendToTerminal={noop} />);
}

function queueButton(container: HTMLElement): HTMLButtonElement | null {
	return container.querySelector<HTMLButtonElement>('button[title*="Queue"]');
}

describe("IdeasPanel — theme-coloured heading", () => {
	it.each(["inline", "detached"] as const)("rejects a platform emoji in the panel heading (%s)", (mode) => {
		const { container } = render(() => (
			<IdeasPanel visible={true} repoPath={null} mode={mode} onClose={noop} onSendToTerminal={noop} />
		));
		const title = container.querySelector("#notes-panel > div > div > span");
		expect(title?.textContent?.trim()).toBe("Ideas");
		expect(title?.querySelector('svg[fill="currentColor"][aria-hidden="true"]')).not.toBeNull();
	});
});

describe("IdeasPanel — IME composition handling", () => {
	beforeEach(() => {
		for (const note of [...ideasStore.state.ideas]) {
			ideasStore.removeIdea(note.id);
		}
		mockInvoke.mockClear();
	});

	it("does not submit when Enter is pressed during IME composition (isComposing=true)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "ねこ" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: true, keyCode: 229 });

		expect(ideasStore.state.ideas.length).toBe(0);
		expect(textarea.value).toBe("ねこ");
	});

	it("does not submit on the IME-confirming Enter even when isComposing has already flipped to false (WebKit quirk)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "猫" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: false, keyCode: 229 });

		expect(ideasStore.state.ideas.length).toBe(0);
		expect(textarea.value).toBe("猫");
	});

	it("does not submit during composition on engines that keep isComposing=true with a non-229 keyCode", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "猫" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: true, keyCode: 13 });

		expect(ideasStore.state.ideas.length).toBe(0);
		expect(textarea.value).toBe("猫");
	});

	it("submits on a real Enter even when keyCode is absent (defensive: undefined !== 229)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "plain text" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: false });

		expect(ideasStore.state.ideas.length).toBe(1);
		expect(ideasStore.state.ideas[0].text).toBe("plain text");
	});

	it("submits normally on a real Enter press (not IME)", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "hello world" } });
		fireEvent.keyDown(textarea, { key: "Enter", isComposing: false, keyCode: 13 });

		expect(ideasStore.state.ideas.length).toBe(1);
		expect(ideasStore.state.ideas[0].text).toBe("hello world");
	});

	it("still inserts a newline on Shift+Enter without submitting", () => {
		const { container } = renderPanel();
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;

		fireEvent.input(textarea, { target: { value: "line one" } });
		fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true, isComposing: false, keyCode: 13 });

		expect(ideasStore.state.ideas.length).toBe(0);
	});
});

describe("IdeasPanel — queue to the agent's Compose queue", () => {
	beforeEach(() => {
		for (const note of [...ideasStore.state.ideas]) {
			ideasStore.removeIdea(note.id);
		}
		mockInvoke.mockClear();
	});

	it("offers no queue action when the host cannot queue", () => {
		ideasStore.addIdea("run the tests");
		const { container } = renderPanel();

		expect(queueButton(container)).toBeNull();
	});

	it("queues the idea instead of typing it, and marks it used", () => {
		ideasStore.addIdea("run the tests");
		const queued: string[] = [];
		const sent: string[] = [];
		const { container } = render(() => (
			<IdeasPanel
				visible={true}
				repoPath={null}
				onClose={noop}
				onSendToTerminal={(text) => sent.push(text)}
				onQueueToTerminal={(text) => queued.push(text)}
			/>
		));

		const button = queueButton(container);
		expect(button).not.toBeNull();
		fireEvent.click(button as HTMLButtonElement);

		expect(queued).toEqual(["run the tests"]);
		// Queueing is a delivery mode, not a different message: it must never
		// also type the idea into the prompt.
		expect(sent).toEqual([]);
		expect(ideasStore.state.ideas[0].usedAt).toBeGreaterThan(0);
	});
});

describe("IdeasPanel — all-repositories toggle", () => {
	const REPO_A = "/repos/alpha";
	const REPO_B = "/repos/beta";

	beforeEach(() => {
		for (const note of [...ideasStore.state.ideas]) {
			ideasStore.removeIdea(note.id);
		}
		ideasStore.addIdea("alpha idea", REPO_A, "alpha");
		ideasStore.addIdea("beta idea", REPO_B, "beta");
		ideasStore.addIdea("global idea", null, null);
		mockInvoke.mockClear();
	});

	function renderForRepo() {
		return render(() => <IdeasPanel visible={true} repoPath={REPO_A} onClose={noop} onSendToTerminal={noop} />);
	}

	function toggle(container: HTMLElement): HTMLButtonElement {
		return container.querySelector<HTMLButtonElement>('button[title="Show ideas from all repositories"]')!;
	}

	it("lists another repository's ideas after the toggle is pressed", () => {
		const { container } = renderForRepo();
		expect(container.textContent).not.toContain("beta idea");

		fireEvent.click(toggle(container));

		expect(container.textContent).toContain("beta idea");
		expect(container.textContent).toContain("alpha idea");
		expect(container.textContent).toContain("global idea");
	});

	it("flips the tooltip and aria-pressed, and a second press restores the repo filter", () => {
		const { container } = renderForRepo();
		const btn = toggle(container);
		expect(btn.getAttribute("aria-pressed")).toBe("false");

		fireEvent.click(btn);
		expect(btn.getAttribute("aria-pressed")).toBe("true");
		expect(btn.title).toBe("Show this repository only");

		fireEvent.click(btn);
		expect(container.textContent).not.toContain("beta idea");
	});

	it("keeps the badge equal to the pending ideas in the visible list", () => {
		const { container } = renderForRepo();
		const badge = () => container.querySelector("[class*=fileCountBadge]")?.textContent;
		expect(badge()).toBe("2");

		fireEvent.click(toggle(container));
		expect(badge()).toBe("3");
	});

	it("tags an idea added in all-mode with the active repository", () => {
		const { container } = renderForRepo();
		fireEvent.click(toggle(container));
		const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
		fireEvent.input(textarea, { target: { value: "added in all mode" } });
		fireEvent.keyDown(textarea, { key: "Enter" });

		const added = ideasStore.state.ideas.find((i) => i.text === "added in all mode");
		expect(added?.repoPath).toBe(REPO_A);
		expect(added?.repoDisplayName).toBe("alpha");
	});

	it("clears only the completed ideas of the visible list", () => {
		for (const i of ideasStore.state.ideas) ideasStore.markUsed(i.id);
		const { container } = renderForRepo();
		const clearBtn = container.querySelector<HTMLButtonElement>('button[title="Clear completed ideas"]')!;
		fireEvent.click(clearBtn);

		expect(ideasStore.state.ideas.map((i) => i.text)).toEqual(["beta idea"]);
	});

	it("clears every repository's completed ideas in all-mode", () => {
		for (const i of ideasStore.state.ideas) ideasStore.markUsed(i.id);
		const { container } = renderForRepo();
		fireEvent.click(toggle(container));
		fireEvent.click(container.querySelector<HTMLButtonElement>('button[title="Clear completed ideas"]')!);

		expect(ideasStore.state.ideas).toEqual([]);
	});
});

// Catches: permanent usage narration replacing concise state and an actionable input.
it("rejects empty Ideas narration while retaining the add control", () => {
	for (const idea of [...ideasStore.state.ideas]) ideasStore.removeIdea(idea.id);
	const { container, getByTitle } = renderPanel();
	expect(container.textContent).not.toContain("Add one below");
	expect(container.textContent).toContain("No ideas yet.");
	expect(container.querySelector("textarea")?.placeholder).toBe("New idea…");
	expect(getByTitle("Add idea (Enter)")).not.toBeNull();
});

it("catches edit image removal or Save losing the draft, and Send/Delete losing their idea", () => {
	for (const idea of [...ideasStore.state.ideas]) ideasStore.removeIdea(idea.id);
	ideasStore.addIdea("Original idea", null, null, ["/img/idea.png"]);
	const onSendToTerminal = vi.fn();
	const view = render(() => <IdeasPanel visible repoPath={null} onClose={noop} onSendToTerminal={onSendToTerminal} />);
	fireEvent.click(view.getByRole("button", { name: "Edit idea" }));
	fireEvent.click(view.getByRole("button", { name: "Remove image" }));
	fireEvent.input(view.getByRole("textbox"), { target: { value: "Revised idea" } });
	fireEvent.click(view.getByRole("button", { name: "Add idea (Enter)" }));
	expect(ideasStore.state.ideas).toHaveLength(1);
	expect(ideasStore.state.ideas[0]).toMatchObject({ text: "Revised idea", images: [] });
	fireEvent.click(view.getByRole("button", { name: "Send to terminal" }));
	expect(onSendToTerminal).toHaveBeenCalledWith("Revised idea");
	expect(ideasStore.state.ideas[0].usedAt).not.toBeNull();
	fireEvent.click(view.getByRole("button", { name: "Delete idea" }));
	expect(ideasStore.state.ideas).toHaveLength(0);
});
