import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { uiStore } from "../../stores/ui";
import type { WorkItemQuery } from "../../types/work-items.gen";
import { listDemo } from "./list/demo";
import { TasksShell } from "./TasksShell";

const disk = vi.hoisted(() => ({ prefs: {} as Record<string, unknown> }));
const [repo, setRepo] = createSignal("/repo-a");
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		state: {
			get activeRepoPath() {
				return repo();
			},
		},
	},
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
		if (command === "load_ui_prefs") return structuredClone(disk.prefs);
		if (command === "save_ui_prefs") disk.prefs = structuredClone(args?.config as Record<string, unknown>);
		return null;
	}),
}));

const query: WorkItemQuery = {
	filters: [],
	savedFilterId: null,
	search: "",
	sort: null,
	groupBy: null,
	cursor: null,
	limit: 100,
};
let registrations: { dispose(): void }[] = [];
beforeEach(async () => {
	disk.prefs = {};
	setRepo("/repo-a");
	await uiStore.hydrate();
	vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
	vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
});
afterEach(() => {
	cleanup();
	uiStore._testCancelPendingSave();
	for (const entry of registrations) entry.dispose();
	registrations = [];
	vi.restoreAllMocks();
});
function register() {
	const a = listDemo(4, "a");
	const b = listDemo(4, "b");
	// These tests cover saved server-query preferences, not the Rust local-projection boundary.
	for (const provider of [a, b]) {
		const list = provider.list.bind(provider);
		provider.list = async (query, signal) => ({ ...(await list(query, signal)), total: 1001 });
	}
	registrations = [workItemProviderRegistry.register(a), workItemProviderRegistry.register(b)];
	return { a, b };
}
function filters() {
	fireEvent.click(screen.getByRole("button", { name: /^Filters/ }));
}

// Catches: persisted stale sort is sent on startup and leaves the list broken rather than restoring the previous accepted order.
it("restored_rejected_sort_falls_back_to_last_valid_sort", async () => {
	const { a } = register();
	// This provider supports the saved order; declare it so the capability-driven picker can render it.
	Object.assign(a, {
		fields: [
			...a.fields,
			{
				id: "updatedAt",
				name: "Updated",
				kind: "date",
				required: false,
				editable: false,
				options: [],
				sortable: true,
				filterable: false,
				groupable: false,
			},
		],
	});
	const list = a.list;
	a.list = async (request, signal) => {
		if (request.sort?.fieldId === "title") throw new Error("Provider cannot sort by title");
		return list(request, signal);
	};
	disk.prefs = {
		tasks_view_preferences: {
			'["/repo-a","demo","a"]': JSON.stringify({
				query: { ...query, sort: { fieldId: "title", descending: false } },
				lastValidSort: { fieldId: "updatedAt", descending: true },
			}),
		},
	};
	await uiStore.hydrate();
	render(() => <TasksShell />);
	await screen.findByRole("button", { name: /Task 00004/ });
	filters();
	await waitFor(() => expect(screen.getByLabelText("Sort tasks")).toHaveValue("updatedAt"));
	expect(screen.getByLabelText("Reverse task sort")).toHaveTextContent("Descending");
	expect(screen.queryByText(/cannot sort/)).not.toBeInTheDocument();
});

// Catches: mount resets or source/repository keys share preferences, and successful settings never reach app persistence.
it("restores_source_view_and_query_after_remount_and_partitions_repositories_and_accounts", async () => {
	register();
	let mounted = render(() => <TasksShell />);
	await screen.findByRole("button", { name: /Task 00004/ });
	fireEvent.change(screen.getByLabelText("Tasks source"), { target: { value: '["demo","b"]' } });
	await waitFor(() => expect(screen.getByLabelText("Task source")).toHaveValue('["demo","b"]'));
	filters();
	fireEvent.change(screen.getByLabelText("Filter Priority"), { target: { value: "high" } });
	fireEvent.change(screen.getByLabelText("Group tasks"), { target: { value: "priority" } });
	fireEvent.change(screen.getByLabelText("Sort tasks"), { target: { value: "title" } });
	fireEvent.click(screen.getByLabelText("Reverse task sort"));
	await screen.findByRole("heading", { name: "High" });
	fireEvent.click(screen.getByRole("tab", { name: "Board" }));
	expect(await screen.findByRole("region", { name: "Tasks board" })).toBeInTheDocument();
	await waitFor(() =>
		expect(Object.values(disk.prefs.tasks_view_preferences as Record<string, string>).join()).toContain('"board"'),
	);
	mounted.unmount();
	await uiStore.hydrate();
	mounted = render(() => <TasksShell />);
	expect(await screen.findByRole("region", { name: "Tasks board" })).toBeInTheDocument();
	expect(screen.getByLabelText("Tasks source")).toHaveValue('["demo","b"]');
	fireEvent.click(screen.getByRole("tab", { name: "List" }));
	await screen.findByRole("heading", { name: "High" });
	filters();
	expect(screen.getByLabelText("Filter Priority")).toHaveValue("high");
	expect(screen.getByLabelText("Group tasks")).toHaveValue("priority");
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("title");
	expect(screen.getByLabelText("Reverse task sort")).toHaveTextContent("Descending");
	fireEvent.change(screen.getByLabelText("Tasks source"), { target: { value: '["demo","a"]' } });
	await screen.findByRole("button", { name: /Task 00004/ });
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("");
	expect(screen.getByLabelText("Filter Priority")).toHaveValue("");
	setRepo("/repo-b");
	await waitFor(() => expect(screen.getByLabelText("Tasks source")).toHaveValue('["demo","a"]'));
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("");
	setRepo("/repo-a");
	fireEvent.change(screen.getByLabelText("Tasks source"), { target: { value: '["demo","b"]' } });
	await screen.findByRole("heading", { name: "High" });
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("title");
	mounted.unmount();
});

// Catches: no saved entry changes the existing default query or view.
it("unsaved_scope_keeps_default_list_and_query", async () => {
	register();
	render(() => <TasksShell />);
	await screen.findByRole("button", { name: /Task 00004/ });
	filters();
	expect(screen.getByRole("tab", { name: "List" })).toHaveAttribute("aria-selected", "true");
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("");
	expect(screen.getByLabelText("Group tasks")).toHaveValue("");
	expect(screen.getByLabelText("Filter Priority")).toHaveValue("");
});

// Catches: a last accepted sort that is no longer supported loops forever instead of trying the provider default.
it("rejected_last_valid_sort_uses_provider_default_and_retains_filters", async () => {
	const { a } = register();
	const list = a.list;
	a.list = async (request, signal) => {
		if (request.sort) throw new Error("Cannot sort by this field");
		return list(request, signal);
	};
	disk.prefs = {
		tasks_view_preferences: {
			'["/repo-a","demo","a"]': JSON.stringify({
				query: {
					...query,
					filters: [{ fieldId: "priority", values: [{ kind: "enum", value: "high" }] }],
					sort: { fieldId: "title", descending: false },
				},
				lastValidSort: { fieldId: "updatedAt", descending: true },
			}),
		},
	};
	await uiStore.hydrate();
	render(() => <TasksShell />);
	await screen.findByRole("button", { name: /Task 00003/ });
	filters();
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("");
	expect(screen.getByLabelText("Filter Priority")).toHaveValue("high");
	expect(screen.queryByRole("button", { name: /Task 00002/ })).not.toBeInTheDocument();
});

// Catches: an unrelated provider outage silently discards the saved sort.
it("network_failure_keeps_saved_sort_and_error_visible", async () => {
	const { a } = register();
	a.list = async () => {
		throw new Error("Network unavailable");
	};
	disk.prefs = {
		tasks_view_preferences: {
			'["/repo-a","demo","a"]': JSON.stringify({
				query: { ...query, sort: { fieldId: "title", descending: false } },
				lastValidSort: { fieldId: "updatedAt", descending: true },
			}),
		},
	};
	await uiStore.hydrate();
	render(() => <TasksShell />);
	await screen.findByText("Network unavailable");
	filters();
	expect(screen.getByLabelText("Sort tasks")).toHaveValue("title");
	expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
	// Preferences are saved only on change (3121), so the failure must leave the saved sort on disk.
	const saved = (disk.prefs.tasks_view_preferences as Record<string, string>)['["/repo-a","demo","a"]'];
	expect(JSON.parse(saved).query.sort).toEqual({ fieldId: "title", descending: false });
});

// Catches: ui-prefs.json merge patches delete null leaves, so a restored query reaches the provider with
// savedFilterId/sort/groupBy undefined instead of the null that the WorkItemQuery contract declares.
it("restored_query_keeps_null_contract_when_disk_dropped_null_leaves", async () => {
	const { a } = register();
	const seen: WorkItemQuery[] = [];
	const list = a.list;
	a.list = async (request, signal) => {
		seen.push(structuredClone(request));
		return list(request, signal);
	};
	disk.prefs = {
		tasks_view_preferences: {
			'["/repo-a","demo","a"]': JSON.stringify({ query: { filters: [], search: "", limit: 100 } }),
		},
	};
	await uiStore.hydrate();
	render(() => <TasksShell />);
	await screen.findByRole("button", { name: /Task 00004/ });
	expect(seen[0]).toMatchObject({ savedFilterId: null, sort: null, groupBy: null, cursor: null });
});
