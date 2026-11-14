import { type Component, createEffect, createSignal, For, on, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { workflowsUi } from "../../stores/workflowsUi";
import { HttpRpcError } from "../../transport";
import { Button } from "../ui/Button";
import s from "./StoriesDialog.module.css";

interface DeliveryRun {
	run_id: string;
	key: string;
	status: { type: string; outcome?: string; reason?: string; diagnostic?: string };
	/** False while a running run waits for a resume after a restart. */
	bound: boolean;
}

const statusText = (run: DeliveryRun): string => {
	if (run.status.type === "running")
		return run.bound
			? t("stories.delivery.running", "Running")
			: t("stories.delivery.waitingResume", "Waiting for resume");
	if (run.status.type === "completed") return run.status.outcome ?? run.status.type;
	return run.status.reason ?? run.status.diagnostic ?? run.status.type;
};

/** Runs the plan and story delivery workflow for one story and lists its runs. The
 *  backend installs the workflow on first use, owns the story snapshot and the run link. */
export const StoryDelivery: Component<{ project: string; storyId: string }> = (props) => {
	const [runs, setRuns] = createSignal<DeliveryRun[]>([]);
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal("");
	let request = 0;
	const call = (action: Record<string, unknown>) =>
		invoke<{ type: string; value: unknown }>("workflow_neutral_action", { project: props.project, action });
	const fail = (cause: unknown) => {
		appLogger.warn("store", "Stories: delivery workflow action failed", { error: String(cause) });
		setError(cause instanceof HttpRpcError ? cause.detail : cause instanceof Error ? cause.message : String(cause));
	};
	async function load(): Promise<void> {
		const current = ++request;
		try {
			const reply = await call({ action: "delivery_runs", story: props.storyId });
			if (reply.type !== "delivery_runs") throw new Error("Invalid delivery runs response");
			if (current === request) setRuns(reply.value as DeliveryRun[]);
		} catch (cause) {
			if (current === request) fail(cause);
		}
	}
	createEffect(
		on(
			() => [props.project, props.storyId],
			() => {
				setRuns([]);
				setError("");
				void load();
			},
		),
	);
	/** A new key starts a run; an existing key resumes that run. */
	async function start(key: string): Promise<void> {
		if (busy()) return;
		setBusy(true);
		setError("");
		try {
			const reply = await call({ action: "start_delivery", story: props.storyId, key });
			if (reply.type !== "inspection") throw new Error("Invalid delivery start response");
			await load();
		} catch (cause) {
			fail(cause);
		} finally {
			setBusy(false);
		}
	}
	return (
		<section aria-label={t("stories.delivery.title", "Delivery workflow")}>
			<h4>{t("stories.delivery.title", "Delivery workflow")}</h4>
			<div class={s.inline}>
				<Button type="button" disabled={busy()} onClick={() => void start(crypto.randomUUID())}>
					{t("stories.delivery.run", "Run delivery workflow")}
				</Button>
				<Button type="button" onClick={() => workflowsUi.open(props.project)}>
					{t("stories.delivery.edit", "Edit in Designer")}
				</Button>
				<Button type="button" disabled={busy()} onClick={() => void load()}>
					{t("stories.refreshPlans", "Refresh")}
				</Button>
			</div>
			<Show when={error()}>
				<p role="alert" class={s.error}>
					{error()}
				</p>
			</Show>
			<Show
				when={runs().length}
				fallback={<p class={s.muted}>{t("stories.delivery.none", "No delivery runs for this story.")}</p>}
			>
				<ul>
					<For each={runs()}>
						{(run) => (
							<li class={s.dependencyRow}>
								<span>
									{run.run_id.slice(0, 8)} · {statusText(run)}
								</span>
								<Show when={run.status.type === "running" && !run.bound}>
									<Button type="button" disabled={busy()} onClick={() => void start(run.key)}>
										{t("stories.delivery.resume", "Resume")}
									</Button>
								</Show>
							</li>
						)}
					</For>
				</ul>
			</Show>
		</section>
	);
};
