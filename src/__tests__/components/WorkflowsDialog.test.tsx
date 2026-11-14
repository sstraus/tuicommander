import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { getActionEntries } from "../../actions/actionRegistry";
import { WorkflowsDialog } from "../../components/WorkflowsDialog/WorkflowsDialog";
import type { ShortcutHandlers } from "../../hooks/useKeyboardShortcuts";
import { revealWaitingItem } from "../../hooks/useWaitingItems";
import { invoke } from "../../invoke";
import { repositoriesStore } from "../../stores/repositories";
import { storiesUi } from "../../stores/storiesUi";
import type { WaitingItem } from "../../stores/waitingItems";
import { workflowsUi } from "../../stores/workflowsUi";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(),
	listen: vi.fn(async () => () => {}),
	guardTauriUnlisten: (fn: () => void) => fn,
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("../../components/WorkflowDesigner/WorkflowDesigner", () => ({
	WorkflowDesigner: (props: { project: string }) => <div>Designer for {props.project}</div>,
}));
afterEach(() => {
	cleanup();
	workflowsUi.close();
	storiesUi.close();
	vi.mocked(invoke).mockReset();
});

describe("Workflows space (Boss Q6=A)", () => {
	// Catches: Workflows requiring a selected plan, or the link to planning lost.
	it("opens the designer without a plan and links to Plans and stories", () => {
		workflowsUi.open("/repo");
		render(() => <WorkflowsDialog project="/repo" onClose={() => workflowsUi.close()} />);
		expect(screen.getByText("Designer for /repo")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Plans and stories" }));
		expect(workflowsUi.visible()).toBe(false);
		expect(storiesUi.visible()).toBe(true);
		expect(storiesUi.project()).toBe("/repo");
	});

	// Catches: a manual check no longer opening its planning item.
	it("opens a waiting manual check in planning", () => {
		const base = { project: "/repo", planId: "plan-1", story: null } as unknown as WaitingItem;
		revealWaitingItem({ ...base, id: "m1", kind: "manual_check" } as WaitingItem);
		expect(workflowsUi.visible()).toBe(false);
		expect(storiesUi.visible()).toBe(true);
		expect(storiesUi.project()).toBe("/repo");
	});

	// Catches: the palette reaching workflows only through planning.
	it("opens Workflows for the active repository from the palette", () => {
		vi.mocked(invoke).mockResolvedValue(undefined);
		const entry = getActionEntries({} as ShortcutHandlers).find((candidate) => candidate.id === "workflows");
		expect(entry?.label).toBe("Workflows");
		repositoriesStore.add({ path: "/repo-workflows", displayName: "Workflows Repo" });
		repositoriesStore.setActive("/repo-workflows");
		entry?.execute();
		expect(workflowsUi.visible()).toBe(true);
		expect(workflowsUi.project()).toBe("/repo-workflows");
	});
});
