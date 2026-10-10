import { createRoot } from "solid-js";
import { describe, expect, it } from "vitest";
import type { AutomationRun, SchedulePreview } from "../../components/AutomationsDialog/contract";
import { createAutomationsStore } from "../../stores/automations";
import { definition, fakeAdapter, runFixture } from "../automationsFixtures";

describe("automations dialog state", () => {
	it("retains the editable prompt after a rejected save instead of losing unsaved work", async () => {
		await createRoot(async (dispose) => {
			const store = createAutomationsStore(
				fakeAdapter({
					save: async () => {
						throw new Error("Invalid run config");
					},
				}),
			);
			await store.refresh();
			store.select(definition);
			store.edit({ prompt: "Keep this draft" });
			await store.save();
			expect(store.draft()?.prompt).toBe("Keep this draft");
			expect(store.error()).toBe("Invalid run config");
			dispose();
		});
	});
	it("ignores an older preview that would validate a different cron than the current edit", async () => {
		await createRoot(async (dispose) => {
			let resolveOld: ((value: SchedulePreview) => void) | undefined;
			const store = createAutomationsStore(
				fakeAdapter({
					preview: async (cron, timezone) =>
						cron === "old"
							? new Promise((resolve) => {
									resolveOld = resolve;
								})
							: { cron, timezone, occurrences: [] },
				}),
			);
			store.select(definition);
			store.edit({ cron: "old" });
			const older = store.preview();
			store.edit({ cron: "new" });
			await store.preview();
			resolveOld?.({ cron: "old", timezone: "Europe/Madrid", occurrences: ["wrong"] });
			await older;
			expect(store.schedulePreview()?.cron).toBe("new");
			dispose();
		});
	});
	it("reports capacity refusal honestly when Run Now was admitted as a skipped run", async () => {
		await createRoot(async (dispose) => {
			const store = createAutomationsStore(fakeAdapter());
			store.select({ ...definition, enabled: false });
			await store.runNow();
			expect(store.notice()).toContain("All slots are busy");
			expect(store.notice()).toContain("skipped_concurrency");
			dispose();
		});
	});
	it("a rejected atomic pause leaves unsaved draft and enabled state intact", async () => {
		await createRoot(async (dispose) => {
			const store = createAutomationsStore(
				fakeAdapter({
					setEnabled: async () => {
						throw new Error("Read-only definitions");
					},
				}),
			);
			store.select(definition);
			store.edit({ prompt: "Unsaved" });
			await store.setEnabled(false);
			expect(store.error()).toBe("Read-only definitions");
			expect(store.draft()?.enabled).toBe(true);
			expect(store.draft()?.prompt).toBe("Unsaved");
			dispose();
		});
	});

	it("does not replace the selected history when an earlier request finishes late", async () => {
		await createRoot(async (dispose) => {
			let finish: ((value: AutomationRun[]) => void) | undefined;
			const oldRuns = [runFixture("old-run", "a1")],
				selectedRuns = [runFixture("selected-run", "a2")];
			const store = createAutomationsStore(
				fakeAdapter({
					history: async (id) =>
						id === "a1"
							? new Promise((resolve) => {
									finish = resolve;
								})
							: selectedRuns,
				}),
			);
			store.select(definition);
			const old = store.loadHistory();
			store.select({ ...definition, id: "a2" });
			await store.loadHistory();
			finish?.(oldRuns);
			await old;
			expect(store.draft()?.id).toBe("a2");
			expect(store.runs()).toEqual(selectedRuns);
			dispose();
		});
	});
});
