import { createEffect, createSignal, lazy, onCleanup, onMount, Show, Suspense } from "solid-js";
import { registerModal } from "../../stores/modalStack";
import {
	answerWorkflowApproval,
	refreshWorkflowApproval,
	subscribeWorkflowApprovals,
	workflowApprovals,
} from "../../stores/workflowApprovals";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import s from "./WorkflowDesigner.module.css";

// The host mounts at startup on desktop and mobile, but the markdown renderer is
// needed only while a question is open. A static import puts the renderer and its
// dependencies in the initial mobile load graph (about 70 kB gzip, over the budget).
const ContentRenderer = lazy(() => import("../ui/ContentRenderer").then((m) => ({ default: m.ContentRenderer })));

/** Shared by desktop and mobile; resolving elsewhere removes the selected question. */
export function WorkflowApprovalHost() {
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const selected = workflowApprovals.selected;
	createEffect(() => {
		const questions = workflowApprovals.pending();
		if (!questions.length) return;
		const due = Math.min(...questions.map((q) => q.expiresAtClient));
		const timer = setTimeout(
			() => {
				for (const question of questions)
					void refreshWorkflowApproval(question.project, question.run).catch((cause) => setError(String(cause)));
			},
			Math.max(1000, Math.min(2_147_483_647, due - Date.now())),
		);
		onCleanup(() => clearTimeout(timer));
	});
	onMount(() => {
		const stop = subscribeWorkflowApprovals();
		void stop.catch((cause) => setError(String(cause)));
		onCleanup(() => {
			void stop.then((fn) => fn()).catch((cause) => setError(String(cause)));
		});
	});
	createEffect(() => {
		const question = selected();
		setError("");
		if (question) registerModal(() => workflowApprovals.open(undefined));
	});
	async function answer(accept: boolean) {
		const question = selected();
		if (!question || busy()) return;
		setBusy(true);
		try {
			await answerWorkflowApproval(question, accept);
		} catch (cause) {
			setError(String(cause));
		} finally {
			setBusy(false);
		}
	}
	return (
		<>
			<Show when={workflowApprovals.pending().length > 0 && !selected()}>
				<div class={s.approvalInbox}>
					<Button onClick={() => workflowApprovals.open(workflowApprovals.pending()[0]?.id)}>
						Workflow approvals ({workflowApprovals.pending().length})
					</Button>
				</div>
			</Show>
			<Show when={selected()}>
				{(question) => (
					<div class={d.overlay}>
						<div class={d.popover} role="dialog" aria-modal="true" aria-label="Workflow approval">
							<div class={d.header}>
								<h4>{question().name} · approval</h4>
							</div>
							<div class={d.body}>
								<p>Run {question().run}</p>
								<Suspense fallback={<p>{question().approval.question.title}</p>}>
									<ContentRenderer content={question().approval.question.title} untrustedContent />
								</Suspense>
								<Show when={question().approval.operator_request}>
									<details>
										<summary>Exact operator request</summary>
										<pre>{JSON.stringify(question().approval.operator_request, null, 2)}</pre>
									</details>
								</Show>
								<Show when={error()}>
									<p role="alert">{error()}</p>
								</Show>
							</div>
							<div class={d.actions}>
								<Button onClick={() => workflowApprovals.open(undefined)}>Later</Button>
								<Button disabled={busy()} onClick={() => void answer(false)}>
									Decline
								</Button>
								<Button variant="primary" disabled={busy()} onClick={() => void answer(true)}>
									Approve
								</Button>
							</div>
						</div>
					</div>
				)}
			</Show>
		</>
	);
}
