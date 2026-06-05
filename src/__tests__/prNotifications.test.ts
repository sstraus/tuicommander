import { afterEach, describe, expect, it } from "vitest";
import { prNotificationsStore } from "../stores/prNotifications";

afterEach(() => {
	// Keep cases isolated and stop the focus timer.
	prNotificationsStore.clearAll();
	prNotificationsStore._testCancelPendingTimers();
});

describe("prNotificationsStore review_started", () => {
	it("adds a review_started entry with the correct shape", () => {
		prNotificationsStore.add({
			repoPath: "/repo",
			branch: "feat/x",
			prNumber: 42,
			title: "Reviewing PR #42",
			type: "review_started",
		});

		const entry = prNotificationsStore.state.notifications.find((n) => n.type === "review_started");
		expect(entry).toBeDefined();
		expect(entry).toMatchObject({
			repoPath: "/repo",
			branch: "feat/x",
			prNumber: 42,
			type: "review_started",
			dismissed: false,
		});
		// Keyed by repo:pr:type so re-fires for the same PR review dedupe.
		expect(entry?.id).toBe("/repo:42:review_started");
	});

	it("deduplicates repeated review_started fires for the same PR", () => {
		const n = {
			repoPath: "/repo",
			branch: "feat/x",
			prNumber: 7,
			title: "Reviewing PR #7",
			type: "review_started" as const,
		};
		prNotificationsStore.add(n);
		prNotificationsStore.add(n);
		const matches = prNotificationsStore.state.notifications.filter((x) => x.id === "/repo:7:review_started");
		expect(matches).toHaveLength(1);
	});
});
