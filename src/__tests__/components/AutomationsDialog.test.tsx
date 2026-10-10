import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { definition, fakeAdapter } from "../automationsFixtures";

describe("AutomationsDialog", () => {
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

	it("contains Tab focus and consumes Escape so the terminal does not receive it", async () => {
		const close = vi.fn();
		render(() => <AutomationsDialog adapter={fakeAdapter()} onClose={close} />);
		await screen.findByText("Sentry");
		const first = screen.getByRole("button", { name: "Close automations" });
		const last = screen.getByRole("button", { name: /Sentry/ });
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
		expect(screen.getByLabelText("Timezone")).toHaveValue("Europe/Madrid");
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
