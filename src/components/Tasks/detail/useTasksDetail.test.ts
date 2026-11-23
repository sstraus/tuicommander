// @vitest-environment jsdom
import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkItemProvider } from "../../../plugins/workItemProvider";
import type { WorkItemDetail, WorkItemRef } from "../../../types/work-items.gen";
import { useTasksDetail } from "./useTasksDetail";

const handlers = new Map<string, Set<(event: { payload: unknown }) => void>>();
vi.mock("../../../invoke", () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		const set = handlers.get(name) ?? new Set();
		set.add(handler);
		handlers.set(name, set);
		return () => set.delete(handler);
	}),
}));
const emit = (name: string, payload: unknown) => {
	for (const handler of handlers.get(name) ?? []) handler({ payload });
};

function fixture(reference: WorkItemRef): { provider: WorkItemProvider; detail: ReturnType<typeof vi.fn> } {
	const value = {
		item: { reference, key: "K", title: "T", itemType: "pull", fields: {}, labels: [], assignees: [] },
		fields: [],
		actions: [],
		comments: { entries: [], nextCursor: null },
	} as unknown as WorkItemDetail;
	const detail = vi.fn(async () => value);
	return {
		provider: {
			providerId: reference.providerId,
			accountId: reference.accountId,
			detail,
		} as unknown as WorkItemProvider,
		detail,
	};
}
const settle = () => vi.advanceTimersByTimeAsync(0);
const mount = (reference: WorkItemRef, provider: WorkItemProvider) =>
	createRoot((dispose) => {
		useTasksDetail({ provider, reference });
		return dispose;
	});
const github = (id: string): WorkItemRef => ({ providerId: "github", accountId: "me", id });

describe("useTasksDetail refresh policy", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		handlers.clear();
	});
	afterEach(() => vi.useRealTimers());

	// Catches: a GitHub pane polling every 60 s, or ignoring the change event for its own repo.
	it("github pr detail refreshes on github-pr-update for its repo and not on timer", async () => {
		const { provider, detail } = fixture(github("acme/app/pull/7"));
		const dispose = mount(github("acme/app/pull/7"), provider);
		await settle();
		expect(detail).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(180_000);
		expect(detail).toHaveBeenCalledTimes(1);
		emit("github-pr-update", {
			repo_path: "/x",
			statuses: [{ number: 1, url: "https://github.com/other/app/pull/1" }],
		});
		await settle();
		expect(detail).toHaveBeenCalledTimes(1);
		emit("github-pr-update", { repo_path: "/x", statuses: [{ number: 7, url: "https://github.com/acme/app/pull/7" }] });
		await settle();
		expect(detail).toHaveBeenCalledTimes(2);
		emit("github-issues-update", {
			repo_path: "/x",
			issues: [{ number: 7, url: "https://github.com/acme/app/issues/7" }],
		});
		await settle();
		expect(detail).toHaveBeenCalledTimes(2);
		dispose();
		emit("github-pr-update", { repo_path: "/x", statuses: [] });
		await settle();
		expect(detail).toHaveBeenCalledTimes(2);
	});

	// Catches: issue panes listening on the PR channel.
	it("github issue detail refreshes on github-issues-update", async () => {
		const { provider, detail } = fixture(github("acme/app/issues/3"));
		const dispose = mount(github("acme/app/issues/3"), provider);
		await settle();
		emit("github-pr-update", { repo_path: "/x", statuses: [] });
		await settle();
		expect(detail).toHaveBeenCalledTimes(1);
		emit("github-issues-update", {
			repo_path: "/x",
			issues: [{ number: 3, url: "https://github.com/acme/app/issues/3" }],
		});
		await settle();
		expect(detail).toHaveBeenCalledTimes(2);
		dispose();
	});

	// Catches: removing the timer for providers that have no change events.
	it("other providers keep the 60 s timer", async () => {
		const reference = { providerId: "jira", accountId: "a", id: "1" };
		const { provider, detail } = fixture(reference);
		const dispose = mount(reference, provider);
		await settle();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(detail).toHaveBeenCalledTimes(2);
		dispose();
	});
});
