import "@testing-library/jest-dom/vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { execute } = vi.hoisted(() => ({ execute: vi.fn() }));
vi.mock("../../hooks/useSmartPrompts", () => ({
	useSmartPrompts: () => ({
		canExecute: (prompt: { name: string }) => ({ ok: prompt.name !== "Unavailable", reason: "Missing context" }),
		executeSmartPrompt: execute,
	}),
}));

import { SmartButtonStrip } from "../../components/SmartButtonStrip/SmartButtonStrip";
import { promptLibraryStore } from "../../stores/promptLibrary";

const prompt = (name: string, enabled = true) =>
	promptLibraryStore.createPrompt({
		name,
		content: "Geometry fixture",
		category: "custom",
		isFavorite: false,
		tags: ["smart", "geometry-test"],
		placement: ["file-context"],
		enabled,
	});
const mount = (props = {}) =>
	render(() => (
		<SmartButtonStrip
			placement="file-context"
			repoPath="/fixture"
			extraFilter={(p) => p.tags?.includes("geometry-test") ?? false}
			{...props}
		/>
	));

beforeEach(async () => {
	await promptLibraryStore.hydrate();
	vi.useFakeTimers();
	vi.clearAllMocks();
	execute.mockResolvedValue({ ok: true });
	for (const p of promptLibraryStore.getAllPrompts())
		if (p.tags?.includes("geometry-test")) promptLibraryStore.deletePrompt(p.id);
});
afterEach(async () => {
	await vi.runOnlyPendingTimersAsync();
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe("SmartButtonStrip shared controls preserve actions", () => {
	it("runs the configured default with its context instead of the first prompt", async () => {
		prompt("First");
		const selected = prompt("Selected default");
		const busy = vi.fn();
		const { getByRole } = mount({
			defaultPromptId: selected.id,
			contextVariables: () => ({ branch: "feature" }),
			onBusyChange: busy,
		});
		fireEvent.click(getByRole("button", { name: "Selected default" }));
		await Promise.resolve();
		expect(busy).toHaveBeenLastCalledWith(false);
		expect(execute).toHaveBeenCalledWith(expect.objectContaining({ id: selected.id }), { branch: "feature" });
	});

	it("runs the chosen menu item and makes it the next main action", async () => {
		prompt("First");
		const chosen = prompt("Second");
		const { getByTitle, getByRole, queryByRole } = mount();
		fireEvent.click(getByTitle("More actions"));
		fireEvent.click(getByRole("button", { name: "Second" }));
		await Promise.resolve();
		expect(execute).toHaveBeenCalledWith(expect.objectContaining({ id: chosen.id }), undefined);
		expect(queryByRole("button", { name: "First" })).toBeNull();
		expect(getByRole("button", { name: "Second" })).toBeInTheDocument();
	});

	it("keeps busy and unavailable actions disabled rather than executing twice", async () => {
		const active = prompt("Active");
		prompt("Unavailable");
		let finish!: (value: { ok: boolean }) => void;
		execute.mockImplementation(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		const { getByRole, getByTitle } = mount({ defaultPromptId: active.id });
		const mainAction = getByRole("button", { name: "Active" });
		fireEvent.click(mainAction);
		expect(mainAction).toBeDisabled();
		fireEvent.click(getByTitle("More actions"));
		expect(getByRole("button", { name: "Unavailable" })).toBeDisabled();
		expect(execute).toHaveBeenCalledTimes(1);
		finish({ ok: true });
		await Promise.resolve();
		expect(mainAction).not.toBeDisabled();
	});

	it("dismisses the menu on an outside click without executing a prompt", () => {
		prompt("First");
		prompt("Other");
		const { getByTitle, queryByRole } = mount();
		fireEvent.click(getByTitle("More actions"));
		expect(queryByRole("button", { name: "Other" })).toBeInTheDocument();
		fireEvent.mouseDown(document.body);
		expect(queryByRole("button", { name: "Other" })).toBeNull();
		expect(execute).not.toHaveBeenCalled();
	});
});
