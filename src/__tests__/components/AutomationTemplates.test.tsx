import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { activityStore } from "../../stores/activityStore";
import { repositoriesStore } from "../../stores/repositories";
import { definition, fakeAdapter } from "../automationsFixtures";

// The application hydrates this owner before mounting the dialog.
beforeAll(() => activityStore.hydrate());

beforeEach(() => {
	// Seeding schedules a debounced save; drop it so no timer outlives the test.
	vi.useFakeTimers();
	for (const path of Object.keys(repositoriesStore.state.repositories)) repositoriesStore.remove(path);
	repositoriesStore.add({ path: "/chosen", displayName: "Chosen" });
	repositoriesStore.add({ path: "/other", displayName: "Other" });
	vi.clearAllTimers();
	vi.useRealTimers();
});

// Catches: applying a template saving/running without consent, dropping backend
// content or selected inputs, and displaying run controls for an unsaved draft.
it("applies a backend draft only after repository and config selection, then saves it paused", async () => {
	const saves: unknown[] = [];
	const runNow = vi.fn(async () => ({ status: "reserved" as const, reason: null }));
	const requests: unknown[] = [];
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({
				profiles: async () => ["my-config"],
				templates: async () => [{ id: "daily_review", name: "Daily change review" }],
				template: async (...args) => {
					requests.push(args);
					return {
						...definition,
						id: "backend-draft",
						name: "Daily change review",
						prompt: "Backend literal prompt",
						repository: args[1],
						run_config: args[2],
						timezone: "Europe/Madrid",
						cron: "0 9 * * *",
						enabled: false,
					};
				},
				save: async (value) => {
					saves.push(structuredClone(value));
					return value;
				},
				runNow,
			})}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByRole("option", { name: "Daily change review" });
	fireEvent.change(screen.getByLabelText("Template"), { target: { value: "daily_review" } });
	expect(screen.getByRole("button", { name: "Apply template" })).toBeDisabled();
	fireEvent.change(screen.getByLabelText("Repository"), { target: { value: "/chosen" } });
	await screen.findByRole("option", { name: "my-config" });
	fireEvent.change(screen.getByLabelText("Run config"), { target: { value: "my-config" } });
	fireEvent.click(screen.getByRole("button", { name: "Apply template" }));
	await waitFor(() => expect(screen.getByLabelText("Prompt")).toHaveValue("Backend literal prompt"));
	// New drafts resolve the backend-local zone through preview before applying templates.
	expect(requests).toEqual([["daily_review", "/chosen", "my-config", "Europe/Madrid"]]);
	expect(screen.getByRole("button", { name: "Timezone" })).toHaveTextContent("Europe/Madrid");
	expect(saves).toHaveLength(0);
	expect(runNow).not.toHaveBeenCalled();
	expect(screen.queryByRole("button", { name: "Run now" })).toBeNull();
	await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled());
	fireEvent.click(screen.getByRole("button", { name: "Save" }));
	await waitFor(() => expect(saves).toHaveLength(1));
	expect(saves[0]).toMatchObject({ repository: "/chosen", run_config: "my-config", enabled: false });
	await screen.findByRole("button", { name: "Resume" });
	expect(runNow).not.toHaveBeenCalled();
});

// Catches: catalogue errors silently looking like an empty list without recovery.
it("shows a failed template catalogue and allows an explicit retry", async () => {
	let fail = true;
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({
				templates: async () => {
					if (fail) throw new Error("Templates unavailable");
					return [{ id: "daily_review", name: "Daily change review" }];
				},
			})}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByText("Templates unavailable");
	fail = false;
	const fieldset = screen.getByText("Built-in templates").closest("fieldset");
	const retry = fieldset?.querySelector("button");
	if (!retry) throw new Error("Template retry missing");
	fireEvent.click(retry);
	await screen.findByRole("option", { name: "Daily change review" });
	expect(screen.queryByText("Templates unavailable")).toBeNull();
});

// Catches: a disabled Apply template button that never says what is missing.
it("shows permanently why a selected template cannot be applied yet", async () => {
	const template = vi.fn();
	render(() => (
		<AutomationsDialog
			onClose={() => {}}
			adapter={fakeAdapter({ templates: async () => [{ id: "daily_review", name: "Daily change review" }], template })}
		/>
	));
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	await screen.findByRole("option", { name: "Daily change review" });
	expect((await screen.findByRole("alert")).textContent).toContain("repository");
	fireEvent.change(screen.getByLabelText("Template"), { target: { value: "daily_review" } });
	expect(screen.getByRole("button", { name: "Apply template" })).toBeDisabled();
	expect(screen.getByRole("alert").textContent).toContain("repository");
	fireEvent.change(screen.getByLabelText("Repository"), { target: { value: "/chosen" } });
	expect(screen.getByRole("alert").textContent).toContain("run config");
	expect(template).not.toHaveBeenCalled();
});

// Catches: the repository field staying free text instead of offering the known repositories.
it("offers the known repositories in the repository picker", async () => {
	render(() => <AutomationsDialog onClose={() => {}} adapter={fakeAdapter()} />);
	fireEvent.click(screen.getByRole("button", { name: "New automation" }));
	const picker = await screen.findByLabelText("Repository");
	expect(picker.tagName).toBe("SELECT");
	expect([...picker.querySelectorAll("option")].map((o) => o.getAttribute("value"))).toEqual(
		expect.arrayContaining(["/chosen", "/other"]),
	);
});

// Catches: the absolutely positioned Saved/error banner covering the last controls of the
// scrolling detail pane, because the pane reserves no room below its content for it.
it("reserves room under the detail pane for the floating banner", () => {
	const css = readFileSync(
		resolve(__dirname, "../../components/AutomationsDialog/AutomationsDialog.module.css"),
		"utf8",
	);
	const banner = /\.dialog \.banner \{[^}]*\bbottom: (\d+)px/.exec(css);
	const detail = /\n\.detail \{[^}]*\bpadding: \d+px \d+px (\d+)px/.exec(css);
	expect(banner && detail).toBeTruthy();
	// A one-line banner is about 48px tall; a wrapped one is taller, so demand clear space beyond it.
	expect(Number(detail?.[1])).toBeGreaterThanOrEqual(Number(banner?.[1]) + 48);
});
