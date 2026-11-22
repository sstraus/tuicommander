import { createSignal } from "solid-js";
import { neutralClient } from "../components/WorkflowDesigner/neutralClient";
import type { Approval, Inspection } from "../components/WorkflowDesigner/neutralInspectionTypes";
import { invoke } from "../invoke";
import { subscribeEvents, type Unsubscribe } from "../transport";
import { activityStore } from "./activityStore";
import { appLogger } from "./appLogger";
import { notificationsStore } from "./notifications";
import { toastsStore } from "./toasts";

export interface WorkflowApproval {
	id: string;
	project: string;
	run: string;
	name: string;
	sequence: number;
	expiresAtClient: number;
	approval: Approval;
}
const [pending, setPending] = createSignal<WorkflowApproval[]>([]);
const [selected, setSelected] = createSignal<string>();
export const workflowApprovals = {
	pending,
	selected: () => pending().find((q) => q.id === selected()),
	open: setSelected,
};
let subscriptionEpoch = 0;
const generations = new Map<string, number>();

function report(cause: unknown) {
	appLogger.warn("app", "Could not refresh workflow approvals", cause);
}
function apply(project: string, run: string, inspection: Inspection) {
	const prefix = `workflow-approval:${project}:${run}:`;
	const previous = pending().filter((q) => q.id.startsWith(prefix));
	const retainedIds = activityStore
		.getActive()
		.filter((item) => item.id.startsWith(prefix))
		.map((item) => item.id);
	const current = (inspection.progress?.approvals ?? []).map(
		(approval): WorkflowApproval => ({
			id: `${prefix}${approval.key.digest}:${approval.question.revision}`,
			project,
			run,
			approval,
			sequence: Number(inspection.snapshot.sequence),
			expiresAtClient:
				Date.now() + Math.max(1000, approval.question.deadline_ms - (inspection.progress?.server_now_ms ?? Date.now())),
			name: inspection.progress?.name ?? "Workflow",
		}),
	);
	for (const id of new Set([...retainedIds, ...previous.map((q) => q.id)]))
		if (!current.some((q) => q.id === id)) {
			activityStore.removeItem(id);
			toastsStore.removeGroup(id);
		}
	setPending((all) => [...all.filter((q) => !q.id.startsWith(prefix)), ...current]);
	if (selected() && !pending().some((q) => q.id === selected())) setSelected(pending()[0]?.id);
	for (const question of current) {
		const open = () => setSelected(question.id);
		activityStore.addItem({
			id: question.id,
			pluginId: "core",
			sectionId: "messages",
			title: `${question.name} needs approval`,
			icon: '<svg viewBox="0 0 16 16" fill="currentColor"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm-1 3h2v6H7zm0 8h2v2H7z"/></svg>',
			subtitle: `${run} · ${question.approval.question.title.split("\n", 1)[0]}`,
			severity: "warn",
			dismissible: false,
			repoPath: project,
			onClick: open,
		});
		if (previous.some((q) => q.id === question.id)) continue;
		toastsStore.add(
			`${question.name} needs approval`,
			`${run} · ${question.approval.question.title.split("\n", 1)[0]}`,
			"warn",
			false,
			{ label: "Review approval", onClick: open },
			undefined,
			project,
			undefined,
			false,
			{ dismissGroup: question.id },
		);
		void notificationsStore.play("attention").catch(report);
		// A real dialog is also available when the Workflows space is closed.
		if (!selected()) open();
	}
}
export async function refreshWorkflowApproval(project: string, run: string) {
	const key = `${project}:${run}`;
	const generation = (generations.get(key) ?? 0) + 1;
	generations.set(key, generation);
	const epoch = subscriptionEpoch;
	const result = await neutralClient(() => project)(
		{ action: "inspect", run_id: run, after_sequence: 0, limit: 1 },
		"inspection",
	);
	if (epoch === subscriptionEpoch && generations.get(key) === generation) apply(project, run, result);
}
async function restore(epoch: number) {
	await activityStore.hydrate();
	if (epoch !== subscriptionEpoch) return;
	const document = await invoke<{ repos?: Record<string, { path?: string }> }>("load_repositories");
	for (const project of Object.keys(document?.repos ?? {})) {
		if (epoch !== subscriptionEpoch) return;
		try {
			const runs = await neutralClient(() => project)({ action: "list_runs" }, "runs");
			if (epoch !== subscriptionEpoch) return;
			const waiting = new Set(runs.filter((r) => r.waiting_on_approval).map((r) => r.id));
			for (const old of pending().filter((q) => q.project === project)) waiting.add(old.run);
			const prefix = `workflow-approval:${project}:`;
			for (const item of activityStore.getActive())
				if (item.id.startsWith(prefix)) waiting.add(item.id.slice(prefix.length).split(":")[0]);
			await Promise.all([...waiting].map((id) => refreshWorkflowApproval(project, id)));
		} catch (cause) {
			report(cause);
		}
	}
}
export async function subscribeWorkflowApprovals(): Promise<Unsubscribe> {
	let disposed = false;
	const epoch = ++subscriptionEpoch;
	const stop = await subscribeEvents(
		{
			"workflow-run-changed": (raw) => {
				const event = raw as { repo_path?: string; payload?: { runId?: string }; __tuic_origin?: unknown };
				// DEFERRED (2026-11-20) — cross-daemon approvals need connection-owned routing.
				// Each daemon's desktop/browser/mobile clients already consume its local events.
				if (event?.__tuic_origin !== undefined) return;
				if (event?.repo_path && event.payload?.runId)
					void refreshWorkflowApproval(event.repo_path, event.payload.runId).catch(report);
			},
		},
		{
			onResync: () => {
				if (!disposed) void restore(epoch).catch(report);
			},
		},
	);
	void restore(epoch).catch(report);
	return () => {
		disposed = true;
		if (subscriptionEpoch === epoch) subscriptionEpoch++;
		stop();
	};
}
export async function answerWorkflowApproval(question: WorkflowApproval, accept: boolean): Promise<void> {
	const { approval, project, run, sequence } = question;
	const call = neutralClient(() => project);
	try {
		await call(
			approval.operator_request
				? {
						action: "operator_answer",
						run_id: run,
						command_id: crypto.randomUUID(),
						expected_sequence: sequence,
						key: approval.key,
						revision: approval.question.revision,
						answer: {
							request: approval.operator_request,
							accept,
							reason: accept ? "Approved by user" : "Declined by user",
						},
					}
				: {
						action: "answer",
						run_id: run,
						command_id: crypto.randomUUID(),
						expected_sequence: sequence,
						key: approval.key,
						revision: approval.question.revision,
						approved: accept,
					},
			"inspection",
		);
	} finally {
		await refreshWorkflowApproval(project, run);
	}
}
