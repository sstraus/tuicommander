import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createRoot } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AutomationsDialog } from "../../components/AutomationsDialog/AutomationsDialog";
import { createAutomationAdapter } from "../../components/AutomationsDialog/transportAdapter";
import { createAutomationsStore } from "../../stores/automations";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup } from "../../transportRuntime";
import { fakeAdapter, runFixture } from "../automationsFixtures";
import { makeTerminal } from "../helpers/store";

describe("automation runs", () => {
	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
	});
	// Catches asynchronous profile options clearing the displayed saved profile while the draft retains it.
	it("keeps the saved profile selected after its profile catalogue arrives", async () => {
		let resolveProfiles!: (names: string[]) => void;
		render(() => (
			<AutomationsDialog
				onClose={() => {}}
				adapter={fakeAdapter({
					profiles: () =>
						new Promise((resolve) => {
							resolveProfiles = resolve;
						}),
				})}
			/>
		));
		const picker = await screen.findByLabelText("Run config");
		expect(picker).toHaveValue("claude:unattended");
		resolveProfiles(["Claude Max", "claude:unattended"]);
		await screen.findByRole("option", { name: "Claude Max" });
		expect(picker).toHaveValue("claude:unattended");
	});

	it("keeps deleted automation snapshots visible and renders backend counts rather than counting the page", async () => {
		const history = vi.fn(async (_id?: string, offset = 0) =>
			offset
				? [runFixture("last", "deleted")]
				: Array.from({ length: 21 }, (_, i) => ({
						...runFixture(`r${i}`, "deleted"),
						status: (["needs_you", "unknown", "interrupted"] as const)[i % 3],
					})),
		);
		const runNow = vi.fn();
		render(() => (
			<AutomationsDialog
				onClose={() => {}}
				adapter={fakeAdapter({
					list: async () => [],
					history,
					runNow,
					summary: async (window) => ({
						total: window === "24h" ? 73 : 401,
						by_status: { needs_you: 11, unknown: 9, interrupted: 4 },
					}),
				})}
			/>
		));
		fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
		await screen.findByText("Total runs: 73");
		expect(screen.getByText("Total runs: 401")).toBeTruthy();
		expect(screen.getAllByText("needs_you").length).toBeGreaterThan(0);
		expect(screen.getAllByText("unknown").length).toBeGreaterThan(0);
		expect(screen.getAllByText("interrupted").length).toBeGreaterThan(0);
		expect(screen.getAllByRole("button", { name: "View run" })).toHaveLength(20);
		fireEvent.click(screen.getAllByRole("button", { name: "View run" })[0]);
		await screen.findByText("Live session unavailable. No replacement run was started.");
		expect(screen.getByText("No workspace was recorded.")).toBeTruthy();
		expect(runNow).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Next runs" }));
		await waitFor(() => expect(screen.getAllByRole("button", { name: "View run" })).toHaveLength(1));
		expect(screen.getByRole("button", { name: "Next runs" })).toBeDisabled();
	});
	it("does not lose saved stdout stderr or truncation evidence when the live session is missing", async () => {
		const run = {
			...runFixture("saved", "deleted"),
			workspace: "/repo/removed-worktree",
			stdout: { text: "Saved report", truncated: true },
			stderr: { text: "Saved error", truncated: false },
		};
		render(() => <AutomationsDialog onClose={() => {}} adapter={fakeAdapter({ history: async () => [run] })} />);
		fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
		fireEvent.click(await screen.findByRole("button", { name: "View run" }));
		expect(screen.getByText("Saved report")).toBeTruthy();
		expect(screen.getByText("Saved error")).toBeTruthy();
		expect(screen.getByText("/repo/removed-worktree")).toBeTruthy();
		expect(screen.getByText("Saved output was truncated by the backend.")).toBeTruthy();
	});
	// Catches View run rendering the 2 KiB list_runs excerpt instead of the saved output `run` returns whole.
	it("View run shows the whole saved output, not the list excerpt", async () => {
		const listed = { ...runFixture("saved", "deleted"), stdout: { text: "tail of report", truncated: true } };
		const whole = { ...listed, stdout: { text: "head of report\ntail of report", truncated: false } };
		const run = vi.fn(async () => whole);
		render(() => (
			<AutomationsDialog onClose={() => {}} adapter={fakeAdapter({ history: async () => [listed], run })} />
		));
		fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
		fireEvent.click(await screen.findByRole("button", { name: "View run" }));
		await screen.findByText(/head of report/);
		expect(screen.queryByText("Saved output was truncated by the backend.")).toBeNull();
	});
	// Catches View run skipping real navigation or selecting a foreign/dead terminal.
	it.each([
		{ owner: "remote-owner", suspended: false, shellState: "idle" as const, navigates: true },
		{ owner: "another-owner", suspended: false, shellState: "idle" as const, navigates: false },
		{ owner: "remote-owner", suspended: true, shellState: "idle" as const, navigates: false },
		{ owner: "remote-owner", suspended: false, shellState: "exited" as const, navigates: false },
	])("View run resolves actual focus or saved output for $owner/$suspended/$shellState", async (row) => {
		const local = "/runs-test-local",
			remote = "/runs-test-remote";
		const localTerminal = "runs-test-local",
			remoteTerminal = "runs-test-remote";
		repositoriesStore.add({ path: local, displayName: "Local" });
		repositoriesStore.add({ path: remote, displayName: "Remote", connectionId: "remote-owner" });
		for (const [path, id, session] of [
			[local, localTerminal, "local-session"],
			[remote, remoteTerminal, "remote-session"],
		]) {
			repositoriesStore.setWorkspace(path, "main");
			terminalsStore.register(id, makeTerminal({ sessionId: session, cwd: path }));
			repositoriesStore.addTerminalToWorkspace(path, "main", id);
		}
		terminalsStore.update(remoteTerminal, { suspended: row.suspended, shellState: row.shellState });
		repositoriesStore.setActive(local);
		terminalsStore.setActive(localTerminal);
		const run = {
			...runFixture("remote-run", "deleted"),
			session_id: "remote-session",
			stdout: { text: "Retained remote output", truncated: false },
		};
		const view = render(() => (
			<AutomationsDialog
				connectionId={row.owner}
				onClose={() => {}}
				adapter={fakeAdapter({ history: async () => [run] })}
			/>
		));
		try {
			fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
			fireEvent.click(await screen.findByRole("button", { name: "View run" }));
			if (row.navigates) {
				expect(terminalsStore.state.activeId).toBe(remoteTerminal);
				expect(repositoriesStore.state.activeRepoPath).toBe(remote);
				expect(repositoriesStore.state.repositories[remote].activeWorkspaceId).toBe("main");
				expect(screen.queryByText("Retained remote output")).toBeNull();
			} else {
				expect(await screen.findByText("Retained remote output")).toBeTruthy();
				expect(terminalsStore.state.activeId).toBe(localTerminal);
				expect(repositoriesStore.state.activeRepoPath).toBe(local);
			}
		} finally {
			view.unmount();
			terminalsStore.remove(localTerminal);
			terminalsStore.remove(remoteTerminal);
			repositoriesStore.remove(local);
			repositoriesStore.remove(remote);
		}
	});
	// Catches an owner being lost between the adapter and real HTTP transport.
	it("sends remote history summaries and canonical run lookup to the owning daemon HTTP endpoint", async () => {
		setRemoteBaseUrlLookup((id) => (id === "remote-owner" ? "http://owner.test:9877" : undefined));
		setRemoteTokenLookup((id) => (id === "remote-owner" ? "owner-token" : undefined));
		const fetch = vi
			.fn()
			.mockImplementation(async () => new Response("[]", { headers: { "content-type": "application/json" } }));
		vi.stubGlobal("fetch", fetch);
		try {
			const adapter = createAutomationAdapter("remote-owner");
			await adapter.history(undefined, 20, 21);
			await adapter.history(undefined, 20, 21, true);
			await adapter.summary("7d");
			await adapter.run("saved-remote-run");
			expect(fetch.mock.calls.map(([url, init]) => [url, init.method, JSON.parse(init.body)])).toEqual([
				[
					"http://owner.test:9877/automations/action?token=owner-token",
					"POST",
					{ input: { action: "list_runs", offset: 20, limit: 21 } },
				],
				[
					"http://owner.test:9877/automations/action?token=owner-token",
					"POST",
					{ input: { action: "list_runs", offset: 20, limit: 21, group_failures: true } },
				],
				[
					"http://owner.test:9877/automations/action?token=owner-token",
					"POST",
					{ input: { action: "summary", window: "7d" } },
				],
				[
					"http://owner.test:9877/automations/action?token=owner-token",
					"POST",
					{ input: { action: "run", id: "saved-remote-run" } },
				],
			]);
		} finally {
			setRemoteBaseUrlLookup(() => undefined);
			setRemoteTokenLookup(() => undefined);
			vi.unstubAllGlobals();
		}
	});
	it("ignores a stale history page so late previous requests cannot replace the requested page", async () => {
		await createRoot(async (dispose) => {
			let resolve!: (runs: ReturnType<typeof runFixture>[]) => void;
			const store = createAutomationsStore(
				fakeAdapter({
					history: async (_id, offset) =>
						offset === 0
							? new Promise((r) => {
									resolve = r;
								})
							: [runFixture("new", "a")],
				}),
			);
			const older = store.loadRuns();
			await store.loadRuns(20);
			resolve([runFixture("old", "a")]);
			await older;
			expect(store.allRuns()[0].id).toBe("new");
			expect(store.runsOffset()).toBe(20);
			dispose();
		});
	});
});

// Catches: legacy failures having no repair path, even when the definition survives.
it("legacy failure offers profile repair", async () => {
	const run = {
		...runFixture("legacy", "a1"),
		status: "failed" as const,
		reason: "Automation run configuration 'claude' not found",
	};
	render(() => <AutomationsDialog onClose={() => {}} adapter={fakeAdapter({ history: async () => [run] })} />);
	fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
	fireEvent.click(await screen.findByRole("button", { name: "Fix" }));
	const picker = await screen.findByRole("combobox", { name: "Run config" });
	expect(picker).toHaveFocus();
	fireEvent.change(picker, { target: { value: "Claude Max" } });
	expect(picker).toHaveValue("Claude Max");
});
