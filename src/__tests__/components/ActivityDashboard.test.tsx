import { render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { ActivityDashboard, type TerminalRow } from "../../components/ActivityDashboard/ActivityDashboard";
import { snapshotToRows } from "../../panelAdapters/activity";
import { activityDashboardStore } from "../../stores/activityDashboard";
import type { ActivitySnapshot } from "../../utils/activitySnapshot";
import "../mocks/tauri";

function row(id: string, subAgentTag: string | null): TerminalRow {
	return {
		id,
		name: id,
		project: null,
		projectColor: undefined,
		agent: "codex",
		status: { label: "Idle", className: "" },
		isWorking: false,
		lastDataAt: null,
		idleSince: null,
		lastPrompt: null,
		agentIntent: null,
		currentTask: null,
		activeSubTasks: 0,
		isActive: false,
		isPromoted: false,
		subAgentTag,
	};
}

describe("Activity Dashboard subagent marker", () => {
	afterEach(() => activityDashboardStore.close());

	it("shows the shared robot icon and parent tooltip without a text arrow", () => {
		activityDashboardStore.open();
		const { container } = render(() => <ActivityDashboard terminals={() => [row("worker", "COORDINATOR")]} />);
		const icon = screen.getByRole("img", { name: "Spawned by COORDINATOR" });
		expect(icon.querySelector("svg")?.getAttribute("width")).toBe("11");
		expect(icon.querySelector("svg")?.getAttribute("height")).toBe("11");
		expect(container.textContent).not.toContain("↳");
	});

	it("preserves the icon through the detached snapshot adapter", () => {
		const snap: ActivitySnapshot = {
			terminals: [
				{
					id: "worker",
					name: "worker",
					shellState: "idle",
					awaitingInput: null,
					sessionId: "pty-worker",
					agentType: "codex",
					agentIntent: null,
					currentTask: null,
					lastPrompt: null,
					activeSubTasks: 0,
					cwd: "/repo",
					project: "repo",
					lastDataAt: null,
					idleSince: null,
					isActive: false,
					isRateLimited: false,
					agentState: null,
					backgroundWork: false,
					isBusy: false,
					isPromoted: false,
					subAgentTag: "COORDINATOR",
				},
			],
		};
		const rows = snapshotToRows(snap);
		const { container } = render(() => <ActivityDashboard embedded terminals={() => rows} />);
		expect(screen.getByRole("img", { name: "Spawned by COORDINATOR" })).not.toBeNull();
		expect(container.textContent).not.toContain("↳");
	});

	it("identifies an external parent and leaves an ordinary terminal unmarked", () => {
		render(() => (
			<ActivityDashboard embedded terminals={() => [row("worker", "external agent"), row("plain", null)]} />
		));
		expect(screen.getByRole("img", { name: "Spawned by external agent" })).not.toBeNull();
		expect(screen.getAllByRole("img", { name: /Spawned by/ })).toHaveLength(1);
	});
});

// Catches: the overlay footer reintroduces always-visible interaction instructions.
it("renders the activity dashboard without invented guidance", () => {
	activityDashboardStore.open();
	const view = render(() => <ActivityDashboard terminals={() => []} />);
	expect(view.container.textContent).toContain("0 terminal(s)");
	expect(view.container.textContent).not.toContain("Click to switch • Esc to close");
	activityDashboardStore.close();
});
