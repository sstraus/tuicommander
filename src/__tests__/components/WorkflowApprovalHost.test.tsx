// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { blankDefinition } from "../../components/WorkflowDesigner/neutralClient";
import type { Inspection } from "../../components/WorkflowDesigner/neutralInspectionTypes";
import { WorkflowApprovalHost } from "../../components/WorkflowDesigner/WorkflowApprovalHost";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";
import { notificationsStore } from "../../stores/notifications";
import { toastsStore } from "../../stores/toasts";

const handlers: Record<string, (payload: unknown) => void> = {};
vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../stores/notifications", () => ({ notificationsStore: { play: vi.fn(async () => {}) } }));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async (callbacks) => {
		Object.assign(handlers, callbacks);
		return () => {};
	}),
}));
let inspection: Inspection;
let listed = true;
beforeEach(async () => {
	vi.useFakeTimers({ shouldAdvanceTime: true });
	listed = true;
	vi.mocked(notificationsStore.play).mockClear();
	inspection = {
		snapshot: { sequence: 12 },
		events: [],
		progress: {
			name: "Release",
			server_now_ms: Date.now(),
			node_states: { start: "waiting" },
			waiting_on_approval: true,
			started_ms: 1,
			updated_ms: 2,
			publication: {
				format_version: 1,
				id: "release",
				project: "/repo",
				revision: 2,
				definition: blankDefinition(),
				editor: { labels: {}, presets: {} },
				pins: { pins: [] },
				subflow_pins: [],
			},
			approvals: [
				{
					key: { fence: { run: "run-1", execution: "root", activation: 1, attempt: 1, scope: null }, digest: "exact" },
					question: {
						revision: 3,
						title: "Publish?\n\n**Evidence**: all checks passed",
						deadline_ms: Date.now() + 60_000,
					},
					operator_request: null,
				},
			],
		},
	};
	vi.mocked(invoke)
		.mockReset()
		.mockImplementation(async (command, args) => {
			if (command === "load_repositories") return { repos: { "/repo": {} } };
			if (command !== "workflow_neutral_action") return [];
			const action = args?.action as { action: string };
			if (action.action === "list_runs")
				return { type: "runs", value: listed ? [{ id: "run-1", waiting_on_approval: true }] : [] };
			if (action.action === "answer") {
				if (inspection.progress) inspection.progress.approvals = [];
				return { type: "inspection", value: structuredClone(inspection) };
			}
			if (action.action === "inspect") return { type: "inspection", value: structuredClone(inspection) };
			throw new Error(`Unexpected ${action.action}`);
		});
	await activityStore.hydrate();
});

// Catches: a retained approval stays invisible with the Designer closed or answers without its fences.
it("restores a real Markdown dialog and answers the exact retained question", async () => {
	render(() => <WorkflowApprovalHost />);
	const dialog = await screen.findByRole("dialog", { name: "Workflow approval" });
	expect(dialog).toHaveTextContent("Publish?");
	// The renderer loads on demand, so the Markdown arrives after the dialog.
	expect((await screen.findByText("Evidence")).tagName).toBe("STRONG");
	expect(activityStore.getActive().some((item) => item.title === "Release needs approval")).toBe(true);
	expect(notificationsStore.play).toHaveBeenCalledWith("attention");
	expect(toastsStore.toasts.find((toast) => toast.title === "Release needs approval")?.message).toBe(
		"run-1 · Publish?",
	);
	fireEvent.click(screen.getByRole("button", { name: "Approve" }));
	await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
	expect(
		vi.mocked(invoke).mock.calls.find(([, args]) => (args?.action as { action?: string })?.action === "answer")?.[1],
	).toMatchObject({
		project: "/repo",
		action: {
			action: "answer",
			run_id: "run-1",
			expected_sequence: 12,
			revision: 3,
			key: { digest: "exact" },
			approved: true,
		},
	});
	expect(activityStore.getActive().some((item) => item.title === "Release needs approval")).toBe(false);
});

// Catches: Later loses the pending request or another client's answer leaves a stale modal or badge.
it("keeps a pending count after Later and closes when another client settles it", async () => {
	render(() => <WorkflowApprovalHost />);
	await screen.findByRole("dialog");
	fireEvent.click(screen.getByRole("button", { name: "Later" }));
	fireEvent.click(screen.getByRole("button", { name: "Workflow approvals (1)" }));
	expect(screen.getByRole("dialog")).toBeInTheDocument();
	if (inspection.progress) inspection.progress.approvals = [];
	handlers["workflow-run-changed"]({ repo_path: "/repo", payload: { runId: "run-1" } });
	await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
	expect(screen.queryByRole("button", { name: /Workflow approvals/ })).not.toBeInTheDocument();
});

// Catches: an ahead-of-server client clock spins zero-delay inspections or prematurely drops an approval.
it("uses the server clock for expiry and retracts after the server deadline", async () => {
	if (!inspection.progress) throw new Error("Missing progress");
	const serverNow = Date.now() - 3_600_000;
	inspection.progress.server_now_ms = serverNow;
	inspection.progress.approvals[0].question.deadline_ms = serverNow + 60_000;
	render(() => <WorkflowApprovalHost />);
	await screen.findByRole("dialog");
	const reads = () =>
		vi.mocked(invoke).mock.calls.filter(([, args]) => (args?.action as { action?: string })?.action === "inspect")
			.length;
	const initial = reads();
	await vi.advanceTimersByTimeAsync(500);
	expect(reads()).toBe(initial);
	inspection.progress.server_now_ms = serverNow + 60_000;
	inspection.progress.approvals = [];
	await vi.advanceTimersByTimeAsync(60_000);
	await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
	expect(reads()).toBe(initial + 1);
});

// Catches: a persisted non-dismissible bell request survives an answer received while the client was offline.
it("reconciles a retained bell request even outside the recent run list", async () => {
	listed = false;
	if (inspection.progress) inspection.progress.approvals = [];
	activityStore.addItem({
		id: "workflow-approval:/repo:run-1:old:1",
		pluginId: "core",
		sectionId: "messages",
		title: "Old approval",
		icon: "",
		dismissible: false,
		repoPath: "/repo",
	});
	render(() => <WorkflowApprovalHost />);
	await waitFor(() => expect(activityStore.getActive().some((item) => item.title === "Old approval")).toBe(false));
	expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

// Flush the real activity store persistence debounce after the mounted host is disposed.
afterEach(async () => {
	cleanup();
	await vi.runOnlyPendingTimersAsync();
	vi.useRealTimers();
});
