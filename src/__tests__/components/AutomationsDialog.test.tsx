import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { activityStore } from "../../stores/activityStore";
import { definition, fakeAdapter } from "../automationsFixtures";

// The real app hydrates this shared owner before mounting dialogs.
beforeAll(() => activityStore.hydrate());

describe("AutomationsDialog", () => {
	// Catches: existing definitions becoming opted in, or checkbox edits lost on save.
	it("keeps close-on-finish off by default and saves both checkbox choices", async () => {
		const saves: unknown[] = [];
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					save: async (value) => {
						saves.push(structuredClone(value));
						return value;
					},
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		const checkbox = screen.getByLabelText("Close the run tab after confirmed completion");
		expect(checkbox).not.toBeChecked();
		for (const enabled of [true, false]) {
			fireEvent.click(checkbox);
			fireEvent.click(screen.getByRole("button", { name: "Save" }));
			await waitFor(() => expect(saves.at(-1)).toMatchObject({ close_on_finish: enabled }));
			await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled());
		}
	});
	// Catches: context enabled without consent, the opt-in lost on save, or prompt whitespace/template text altered.
	it("saves explicit precheck context consent while preserving the exact original prompt", async () => {
		const original = "  Literal {{files}}\nKeep é and trailing spaces.  \n";
		const saves: unknown[] = [];
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					list: async () => [{ definition: { ...definition, prompt: original }, next_run_ms: null, last_status: null }],
					save: async (value) => {
						saves.push(structuredClone(value));
						return value;
					},
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		fireEvent.click(screen.getByLabelText("Run a shell precheck before scheduled runs"));
		const consent = screen.getByLabelText("Include precheck stdout in the agent prompt");
		expect(consent).not.toBeChecked();
		fireEvent.input(screen.getByLabelText("Precheck command"), { target: { value: "git status --short" } });
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(saves).toHaveLength(1));
		await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled());
		expect(saves[0]).toMatchObject({ prompt: original, precheck: { inject_output: false } });
		fireEvent.click(screen.getByLabelText("Include precheck stdout in the agent prompt"));
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(saves).toHaveLength(2));
		await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled());
		expect(saves[1]).toMatchObject({
			prompt: original,
			precheck: { command: "git status --short", timeout_secs: 30, inject_output: true },
		});
		expect(screen.getByLabelText("Include precheck stdout in the agent prompt")).toBeChecked();
		fireEvent.click(screen.getByLabelText("Run a shell precheck before scheduled runs"));
		fireEvent.click(screen.getByLabelText("Run a shell precheck before scheduled runs"));
		expect(screen.getByLabelText("Include precheck stdout in the agent prompt")).not.toBeChecked();
	});
	it("previews and saves Once as a local wall time without converting through the browser zone", async () => {
		let previewed: unknown, saved: unknown;
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					previewDefinition: async (value) => {
						previewed = value;
						return {
							cron: "",
							timezone: "Europe/Madrid",
							once_local: value.once_local ?? null,
							occurrences: ["2026-10-16T08:00:00Z"],
							completed: false,
						};
					},
					save: async (value) => {
						saved = value;
						return value;
					},
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		fireEvent.change(screen.getByLabelText("Cadence"), { target: { value: "once" } });
		fireEvent.input(screen.getByLabelText("Once local time"), { target: { value: "2026-10-16T10:00" } });
		fireEvent.click(screen.getByRole("button", { name: "Preview schedule" }));
		await waitFor(() =>
			expect(previewed).toMatchObject({ cron: "", once_local: "2026-10-16T10:00:00", timezone: "Europe/Madrid" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		await waitFor(() => expect(saved).toMatchObject({ cron: "", once_local: "2026-10-16T10:00:00" }));
	});
	it("shows an exhausted Once from backend evidence instead of promising another occurrence", async () => {
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					list: async () => [
						{
							definition: { ...definition, cron: "", once_local: "2026-10-16T10:00:00" },
							next_run_ms: null,
							last_status: "completed",
						},
					],
					previewDefinition: async (value) => ({
						cron: "",
						timezone: value.timezone,
						once_local: value.once_local ?? null,
						occurrences: [],
						completed: true,
					}),
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		await screen.findByText("This one-time occurrence has completed.");
		expect(screen.getByLabelText("Cadence")).toHaveValue("once");
	});

	// Catches: selecting a zone changing only the label, not the draft sent to preview.
	it("filters timezone choices and previews the selected zone without replacing an unknown saved zone", async () => {
		const preview = vi.fn(async (cron: string, timezone: string) => ({ cron, timezone, occurrences: [] }));
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					list: async () => [
						{ definition: { ...definition, timezone: "Saved/Unknown" }, next_run_ms: null, last_status: null },
					],
					preview,
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		const trigger = screen.getByRole("button", { name: "Timezone" });
		expect(trigger).toHaveTextContent("Saved/Unknown");
		fireEvent.click(trigger);
		expect(screen.getByRole("menuitem", { name: "Saved/Unknown" })).toHaveAttribute("data-selected", "true");
		fireEvent.input(screen.getByRole("textbox", { name: "Timezone" }), { target: { value: "new_york" } });
		expect(screen.queryByRole("menuitem", { name: "Europe/Madrid" })).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("menuitem", { name: "America/New_York" }));
		expect(trigger).toHaveTextContent("America/New_York");
		fireEvent.click(screen.getByRole("button", { name: "Preview schedule" }));
		await waitFor(() => expect(preview).toHaveBeenLastCalledWith(definition.cron, "America/New_York"));
	});

	it("contains Tab focus and consumes Escape so the terminal does not receive it", async () => {
		const close = vi.fn();
		render(() => <AutomationsDialog adapter={fakeAdapter()} onClose={close} />);
		await screen.findByText("Sentry");
		const first = screen.getByRole("button", { name: "Close automations" });
		await screen.findByLabelText("Name");
		const controls = screen
			.getByRole("dialog")
			.querySelectorAll<HTMLElement>(
				'button:not(:disabled):not([tabindex="-1"]), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]',
			);
		const last = [...controls].at(-1)!;
		last.focus();
		fireEvent.keyDown(last, { key: "Tab" });
		expect(document.activeElement).toBe(first);
		first.focus();
		fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
		expect(document.activeElement).toBe(last);
		fireEvent.keyDown(last, { key: "Escape" });
		expect(close).toHaveBeenCalledOnce();
	});
	it("searches displayed names and repositories without hiding backend load failures", async () => {
		render(() => <AutomationsDialog adapter={fakeAdapter()} onClose={() => {}} />);
		await screen.findByText("Sentry");
		fireEvent.input(screen.getByLabelText("Search automations"), { target: { value: "missing" } });
		expect(screen.getByText("No matching automations.")).toBeTruthy();
		fireEvent.input(screen.getByLabelText("Search automations"), { target: { value: "/repo" } });
		expect(screen.getByText("Sentry")).toBeTruthy();
	});
	it("shows overlap skips and a named-zone next run rather than promising an extra launch", async () => {
		render(() => <AutomationsDialog adapter={fakeAdapter()} onClose={() => {}} />);
		await screen.findByText("Sentry");
		expect(screen.getByText("skipped_overlap")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: /Sentry/ }));
		expect(screen.getByRole("button", { name: "Timezone" })).toHaveTextContent("Europe/Madrid");
		expect(screen.getByText(/Skip while a run is still open/)).toBeTruthy();
	});
	it("keeps paused Run Now available and displays a real capacity skip", async () => {
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					list: async () => [{ definition: { ...definition, enabled: false }, next_run_ms: null, last_status: null }],
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		fireEvent.click(screen.getByRole("button", { name: "Run now" }));
		await screen.findByText(/All slots are busy/);
	});
	it("does not delete on the first click and preserves the definition when deletion fails", async () => {
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					remove: async () => {
						throw new Error("Disk is read-only");
					},
				})}
				onClose={() => {}}
			/>
		));
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		fireEvent.click(screen.getByRole("button", { name: "Delete" }));
		expect(screen.getByRole("button", { name: "Confirm delete" })).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Confirm delete" }));
		await screen.findByRole("alert");
		expect(screen.getByText("Disk is read-only")).toBeTruthy();
		expect(screen.getByRole("button", { name: /Sentry/ })).toBeTruthy();
	});
	it("uses the backend preset and previews its returned cron rather than computing a local schedule", async () => {
		render(() => <AutomationsDialog adapter={fakeAdapter()} onClose={() => {}} />);
		fireEvent.click(await screen.findByRole("button", { name: /Sentry/ }));
		fireEvent.change(screen.getByLabelText("Cadence"), { target: { value: "weekdays" } });
		fireEvent.click(screen.getByRole("button", { name: "Apply cadence" }));
		await waitFor(() => expect(screen.getByLabelText("Cron")).toHaveValue("30 7 * * 1-5"));
	});
	it("distinguishes an empty list from a failed load and lets the user retry", async () => {
		let failed = true;
		render(() => (
			<AutomationsDialog
				adapter={fakeAdapter({
					list: async () => {
						if (failed) throw new Error("Backend unavailable");
						return [];
					},
				})}
				onClose={() => {}}
			/>
		));
		await screen.findByText("Backend unavailable");
		failed = false;
		fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
		await screen.findByText("No automations yet. Create one to schedule an agent run.");
	});
});
