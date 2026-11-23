import { createResource, createSignal, For, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { workflowsUi } from "../../stores/workflowsUi";
import { HttpRpcError } from "../../transport";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import s from "./PrReviewWorkflowAction.module.css";

/** Backend `PrReviewTarget` (workflows/run/neutral_pr_review.rs). */
export interface PrReviewTarget {
	binding_id: string;
	enabled: boolean;
	publication: { project: string; id: string; revision: number };
	name: string;
}

interface Reply {
	type: string;
	value: unknown;
}

interface StartedRun {
	id: string;
	status: string;
}

const message = (cause: unknown) =>
	cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause);

/** Reads the started run out of the backend inspection reply. */
function startedRun(value: unknown): StartedRun | undefined {
	const snapshot = (value as { snapshot?: { run?: { id?: unknown }; status?: { type?: unknown } } } | null)?.snapshot;
	const id = snapshot?.run?.id;
	if (typeof id !== "string") return undefined;
	return { id, status: typeof snapshot?.status?.type === "string" ? snapshot.status.type : "" };
}

/** Starts the project's published PR review workflow for one pull request. The backend
 *  picks the workflow from the project's pull_request trigger binding and reads the head
 *  and base from GitHub when the user clicks. */
export function PrReviewWorkflowAction(props: { project: string; pullRequestUrl: string }) {
	const call = (action: Record<string, unknown>) =>
		invoke<Reply>("workflow_neutral_action", { project: props.project, action });
	const [targets] = createResource(
		() => ({ project: props.project, url: props.pullRequestUrl }),
		async ({ url }) => {
			try {
				const reply = await call({ action: "pr_review_targets", pull_request: url });
				if (reply.type !== "pr_review_targets") throw new Error("Invalid PR review targets response");
				return { list: reply.value as PrReviewTarget[], error: "" };
			} catch (cause) {
				appLogger.warn("github", "PR review workflow lookup failed", { error: String(cause) });
				return { list: [] as PrReviewTarget[], error: message(cause) };
			}
		},
	);
	const [chosen, setChosen] = createSignal("");
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	const [run, setRun] = createSignal<StartedRun>();
	const list = () => targets()?.list ?? [];
	const binding = () => chosen() || (list().length === 1 ? list()[0].binding_id : "");
	const reason = () => {
		if (targets.loading) return "";
		if (targets()?.error) return t("prReviewWorkflow.unavailable", "PR review workflows are unavailable.");
		if (list().length === 0)
			return t(
				"prReviewWorkflow.none",
				"No PR review workflow: bind a published workflow to a GitHub pull_request trigger in the Workflow Designer.",
			);
		return "";
	};

	async function start() {
		if (busy() || !binding()) return;
		setBusy(true);
		setError("");
		setRun(undefined);
		try {
			const reply = await call({ action: "start_pr_review", pull_request: props.pullRequestUrl, binding: binding() });
			const started = reply.type === "inspection" ? startedRun(reply.value) : undefined;
			if (!started) throw new Error("Invalid PR review start response");
			setRun(started);
		} catch (cause) {
			appLogger.warn("github", "PR review workflow start failed", { error: String(cause) });
			setError(message(cause));
		} finally {
			setBusy(false);
		}
	}

	return (
		<div class={s.action} data-testid="pr-review-workflow">
			<div class={s.row}>
				<Show when={list().length > 1}>
					<Select
						aria-label={t("prReviewWorkflow.choose", "PR review workflow")}
						value={chosen()}
						onChange={(event) => setChosen(event.currentTarget.value)}
					>
						<option value="">{t("prReviewWorkflow.chooseOption", "Choose a workflow…")}</option>
						<For each={list()}>{(target) => <option value={target.binding_id}>{target.name}</option>}</For>
					</Select>
				</Show>
				<Button
					type="button"
					disabled={busy() || targets.loading || !binding()}
					title={reason() || undefined}
					onClick={() => void start()}
				>
					{t("prReviewWorkflow.start", "Review with workflow")}
				</Button>
			</div>
			<Show when={reason()}>
				{(text) => (
					<p class={s.muted} title={targets()?.error || undefined}>
						{text()}
					</p>
				)}
			</Show>
			<Show when={run()}>
				{(started) => (
					<div class={s.row} role="status">
						<span>
							{t("prReviewWorkflow.started", "Review run")} {started().id.slice(0, 8)} · {started().status}
						</span>
						<Button type="button" onClick={() => workflowsUi.openRun(props.project, started().id)}>
							{t("prReviewWorkflow.open", "Open run")}
						</Button>
					</div>
				)}
			</Show>
			<Show when={error()}>
				{(text) => (
					<p class={s.error} role="alert">
						{text()}
					</p>
				)}
			</Show>
		</div>
	);
}
