import { type Component, createResource, createSignal, Show } from "solid-js";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import type { BranchPrStatus, CheckDetail } from "../../types";
import { getCiClass } from "../../utils/ciDisplay";
import { effectiveMergeMethod } from "../../utils/prMerge";
import { preferredMergeStrategy } from "../../utils/prMergeStrategy";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./PrDetailPopover.module.css";

/** Backend `PrActionState` (github_pr_actions.rs); only the fields the buttons read. */
interface PrActionState {
	autoMergeRequest?: unknown;
	mergeQueueEntry?: unknown;
	queueRequired?: boolean;
}

const RUN_ID = /\/actions\/runs\/(\d+)/;

/** Distinct GitHub Actions run ids of the failed checks (their details URL names the run). */
export function failedRunIds(checks: readonly CheckDetail[]): number[] {
	const ids = new Set<number>();
	for (const check of checks) {
		if (getCiClass(check.state) !== "failure") continue;
		const match = RUN_ID.exec(check.html_url);
		if (match) ids.add(Number(match[1]));
	}
	return [...ids];
}

export interface PrActionButtonsProps {
	repoPath: string;
	pr: BranchPrStatus;
	checks: readonly CheckDetail[];
}

/** Rerun failed jobs, auto-merge, merge queue and request changes for an open PR. */
export const PrActionButtons: Component<PrActionButtonsProps> = (props) => {
	const [busy, setBusy] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const [notice, setNotice] = createSignal<string | null>(null);
	const [requesting, setRequesting] = createSignal(false);
	const [body, setBody] = createSignal("");
	const [actionState, { refetch }] = createResource(
		() => ({ repoPath: props.repoPath, prNumber: props.pr.number, head: props.pr.head_ref_oid }),
		({ repoPath, prNumber }) =>
			invoke<PrActionState>("get_pr_action_state", { repoPath, prNumber }).catch((e) => {
				appLogger.warn("github", "Failed to load PR action state", { error: String(e) });
				return null;
			}),
	);
	const open = () => props.pr.state?.toUpperCase() === "OPEN";
	const mergeable = () => open() && !props.pr.is_draft;
	const runIds = () => failedRunIds(props.checks);
	const autoMergeOn = () => Boolean(actionState()?.autoMergeRequest);
	const queueRequired = () => actionState()?.queueRequired === true;
	const queued = () => Boolean(actionState()?.mergeQueueEntry);
	const mergeMethod = () => effectiveMergeMethod(props.pr, preferredMergeStrategy(props.repoPath));

	async function run(success: string, work: () => Promise<unknown>, after?: () => void) {
		if (busy()) return;
		setBusy(true);
		setError(null);
		setNotice(null);
		try {
			await work();
			setNotice(success);
			after?.();
		} catch (e) {
			setError(String(e));
			appLogger.warn("github", "PR action failed", { error: String(e) });
		} finally {
			setBusy(false);
		}
	}

	const rerun = () =>
		run(t("prDetail.rerunAccepted", "Failed jobs re-run requested"), async () => {
			for (const runId of runIds()) await invoke("rerun_failed_pr_jobs", { repoPath: props.repoPath, runId });
		});
	const toggleAutoMerge = () =>
		run(
			autoMergeOn()
				? t("prDetail.autoMergeOff", "Auto-merge disabled")
				: t("prDetail.autoMergeOn", "Auto-merge enabled"),
			() =>
				invoke("set_pr_auto_merge", {
					repoPath: props.repoPath,
					prNumber: props.pr.number,
					enabled: !autoMergeOn(),
					mergeMethod: mergeMethod(),
					expectedHeadSha: props.pr.head_ref_oid,
				}),
			() => void refetch(),
		);
	const toggleQueue = () =>
		run(
			queued()
				? t("prDetail.queueLeft", "Removed from the merge queue")
				: t("prDetail.queueJoined", "Added to the merge queue"),
			() =>
				queued()
					? invoke("set_pr_merge_queue", {
							repoPath: props.repoPath,
							prNumber: props.pr.number,
							enqueued: false,
							expectedHeadSha: props.pr.head_ref_oid,
						})
					: invoke("merge_or_enqueue_pr", {
							repoPath: props.repoPath,
							prNumber: props.pr.number,
							mergeMethod: mergeMethod(),
							expectedHeadSha: props.pr.head_ref_oid,
						}),
			() => void refetch(),
		);
	const requestChanges = () =>
		run(
			t("prDetail.changesRequested", "Changes requested"),
			() =>
				invoke("post_pr_review", {
					repoPath: props.repoPath,
					prNumber: props.pr.number,
					body: body().trim(),
					event: "REQUEST_CHANGES",
					comments: [],
				}),
			() => {
				setBody("");
				setRequesting(false);
			},
		);

	return (
		<Show when={open()}>
			<div class={s.actions} data-testid="pr-action-buttons">
				<Show when={runIds().length > 0}>
					<Button type="button" disabled={busy()} onClick={rerun}>
						{t("prDetail.rerunFailed", "Re-run failed jobs")}
					</Button>
				</Show>
				<Show when={mergeable()}>
					<Button type="button" disabled={busy() || actionState.loading} onClick={toggleAutoMerge}>
						{autoMergeOn()
							? t("prDetail.disableAutoMerge", "Disable auto-merge")
							: t("prDetail.enableAutoMerge", "Enable auto-merge")}
					</Button>
					<Show when={queueRequired() || queued()}>
						<Button type="button" disabled={busy()} onClick={toggleQueue}>
							{queued()
								? t("prDetail.leaveQueue", "Remove from merge queue")
								: t("prDetail.joinQueue", "Add to merge queue")}
						</Button>
					</Show>
				</Show>
				<Button type="button" disabled={busy()} onClick={() => setRequesting((value) => !value)}>
					{t("prDetail.requestChanges", "Request changes")}
				</Button>
				<Show when={requesting()}>
					<form
						class={s.requestForm}
						onSubmit={(event) => {
							event.preventDefault();
							if (body().trim()) void requestChanges();
						}}
					>
						<Textarea
							aria-label={t("prDetail.requestChangesBody", "Review summary")}
							placeholder={t("prDetail.requestChangesBody", "Review summary")}
							value={body()}
							onInput={(event) => setBody(event.currentTarget.value)}
						/>
						<Button type="submit" disabled={busy() || !body().trim()}>
							{t("prDetail.submitRequestChanges", "Submit")}
						</Button>
					</form>
				</Show>
				<Show when={notice()}>{(text) => <div role="status">{text()}</div>}</Show>
				<Show when={error()}>
					{(text) => (
						<div class={s.aiReviewError} role="alert">
							{text()}
						</div>
					)}
				</Show>
			</div>
		</Show>
	);
};
