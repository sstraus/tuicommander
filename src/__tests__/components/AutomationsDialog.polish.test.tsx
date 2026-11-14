import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { activityStore } from "../../stores/activityStore";
import { automationsUi } from "../../stores/automations";
import { definition, fakeAdapter } from "../automationsFixtures";

beforeAll(() => activityStore.hydrate());

afterEach(() => {
	cleanup();
	localStorage.clear();
	automationsUi.open();
});
const mount = (cron = "30 3 * * *", repository = "/Users/boss/Gits/tuicommander/") =>
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({
				list: async () => [
					{ definition: { ...definition, cron, repository }, next_run_ms: 1791568800000, last_status: null },
				],
			})}
		/>
	));

describe("Automations dialog readability", () => {
	// Catches: full paths and opaque cron replacing the useful card summary.
	it.each([
		["30 3 * * *", "Every day at 03:30"],
		["15 * * * *", "Every hour at :15"],
		["*/5 * * * *", "Every 5 minutes"],
		["* * * * *", "Every minute"],
		["*/7 * * * *", "*/7 * * * *"],
		["0 9 * * 1", "Every Monday at 09:00"],
		["0 9 * * 7", "Every Sunday at 09:00"],
		["0 9 1 * 1", "0 9 1 * 1"],
		["*/90 * * * *", "*/90 * * * *"],
	])("renders %s as %s without inventing a next occurrence", async (cron, text) => {
		mount(cron);
		const card = await screen.findByRole("button", { name: /Sentry/ });
		expect(within(card).getByText("tuicommander")).toHaveAttribute("title", "/Users/boss/Gits/tuicommander/");
		expect(within(card).getByText(text)).toBeTruthy();
		expect(card.querySelector("time")).toHaveAttribute("datetime", new Date(1791568800000).toISOString());
	});
	// Catches: a loaded list leaving the editor empty and refresh discarding unsaved edits.
	it("opens the first definition and preserves the draft when refreshing", async () => {
		mount();
		expect(await screen.findByLabelText("Name")).toHaveValue("Sentry");
		expect(screen.queryByRole("checkbox", { name: "Select first automation on open" })).not.toBeInTheDocument();
		fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Unsaved" } });
		fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
		await waitFor(() => expect(screen.getByRole("button", { name: "Refresh" })).not.toBeDisabled());
		expect(screen.getByLabelText("Name")).toHaveValue("Unsaved");
	});
	// Catches: late initial loading replacing a new automation the user already started.
	it("keeps a new draft started before the list finishes loading", async () => {
		let resolve!: (items: Awaited<ReturnType<ReturnType<typeof fakeAdapter>["list"]>>) => void;
		render(() => (
			<AutomationsDialog
				onClose={() => {}}
				adapter={fakeAdapter({
					list: () =>
						new Promise((r) => {
							resolve = r;
						}),
				})}
			/>
		));
		fireEvent.click(screen.getByRole("button", { name: "New automation" }));
		fireEvent.input(screen.getByLabelText("Name"), { target: { value: "New draft" } });
		resolve([{ definition, next_run_ms: null, last_status: null }]);
		await screen.findByRole("button", { name: /Sentry/ });
		expect(screen.getByLabelText("Name")).toHaveValue("New draft");
	});
	// Catches: tabs lacking roving keyboard focus and an associated visible panel.
	it("navigates tabs with arrows and Home/End", async () => {
		mount();
		const definitions = screen.getByRole("tab", { name: "Automation definitions" });
		const runs = screen.getByRole("tab", { name: "Runs" });
		expect(definitions).toHaveAttribute("aria-selected", "true");
		definitions.focus();
		fireEvent.keyDown(definitions, { key: "ArrowRight" });
		expect(runs).toHaveFocus();
		expect(runs).toHaveAttribute("aria-selected", "true");
		expect(screen.getByRole("tabpanel")).toHaveAttribute("aria-labelledby", runs.id);
		fireEvent.keyDown(runs, { key: "Home" });
		expect(definitions).toHaveFocus();
		fireEvent.keyDown(definitions, { key: "End" });
		expect(runs).toHaveFocus();
		fireEvent.keyDown(runs, { key: "ArrowRight" });
		expect(definitions).toHaveFocus();
	});
});
