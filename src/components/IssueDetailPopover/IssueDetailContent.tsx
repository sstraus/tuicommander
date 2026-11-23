import { type Component, createSignal, For, type JSX, Show } from "solid-js";
import { t } from "../../i18n";
import { openTaskConversation } from "../../services/taskNavigation";
import { autofixBranchName } from "../../stores/autofix";
import { githubStore } from "../../stores/github";
import type { GitHubIssue } from "../../types";
import { cx } from "../../utils";
import { relativeTime } from "../../utils/time";
import { SmartButtonStrip } from "../SmartButtonStrip/SmartButtonStrip";
import na from "../shared/NextAction.module.css";
import { ToolbarIcons } from "../shared/PanelToolbar";
import { ReplyBox } from "../shared/PreviewReply";
import {
	createWorkItemPreview,
	descriptionSummary,
	PreviewComments,
	PreviewDescription,
} from "../shared/WorkItemPreview";
import { createNewComments } from "../shared/workItemVisit";
import { Button } from "../ui/Button";
import { PanelSection } from "../ui/PanelSection";
import s from "./IssueDetailContent.module.css";

export type IssueNextAction = "reopen" | "create-pr" | "work-on-it";

export interface IssueDetailContentProps {
	issue: GitHubIssue;
	repoPath: string;
	/** True when the issue's auto-fix branch has a worktree and no pull request yet. */
	autofixBranchReady?: boolean;
	onReopen?: () => void;
	reopening?: boolean;
	onAutofix?: () => void;
	onCreatePr?: () => void;
	creatingPr?: boolean;
	onOpenBranch?: () => void;
	/** The fixed toolbar and action errors, rendered between the next action and the sections. */
	children?: JSX.Element;
}

/** Issue detail in the UI v3 layout (docs/frontend/ui-v3.md): status row, one next action,
 *  the caller's fixed toolbar, then Description and Comments sections read from the work-item provider. */
export const IssueDetailContent: Component<IssueDetailContentProps> = (props) => {
	const isOpen = () => props.issue.state?.toUpperCase() === "OPEN";
	const nextAction = (): IssueNextAction => {
		if (!isOpen()) return "reopen";
		return props.autofixBranchReady ? "create-pr" : "work-on-it";
	};
	const preview = createWorkItemPreview(() => props.issue.url || undefined);
	// Loaded with the panel: the Description summary shows its length and Assign me needs the provider.
	preview.load();
	const fresh = createNewComments(
		() => props.issue.url || undefined,
		() => props.issue.comments_count,
	);
	const hasNew = () => isOpen() && fresh().count > 0;
	/** New comments make Reply the next action; Work on it stays beside it. */
	const replyLeads = () => hasNew() && nextAction() === "work-on-it" && preview.offers("comment");
	const [assigning, setAssigning] = createSignal(false);
	const [assigned, setAssigned] = createSignal(false);
	const [assignError, setAssignError] = createSignal("");
	const viewer = () => githubStore.state.viewerLogin;
	const canAssignMe = () =>
		!!viewer() && !assigned() && !props.issue.assignees?.includes(viewer() ?? "") && preview.offers("edit_field");
	/** GitHub replaces the whole assignee list, so the current assignees go along with the viewer. */
	const assignMe = async () => {
		const login = viewer();
		if (!login) return;
		setAssigning(true);
		setAssignError("");
		try {
			const user = (name: string) => ({ id: name, name, avatarUrl: null, bot: false });
			await preview.act({
				kind: "edit_field",
				value: {
					fieldId: "assignees",
					value: { kind: "user", value: [...(props.issue.assignees ?? []), login].map(user) },
				},
			});
			setAssigned(true);
		} catch (cause) {
			setAssignError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			setAssigning(false);
		}
	};
	const openConversation = () => {
		if (props.issue.url) void openTaskConversation(props.issue.url);
	};

	return (
		<>
			{/* Meta line: who opened it and when, plus assignees and milestone */}
			<div class={s.meta}>
				<span>
					{t("issueDetail.openedBy", "opened by")} <span class={s.author}>{props.issue.author}</span>
				</span>
				<Show when={props.issue.created_at}>{(when) => <span>· {relativeTime(when())}</span>}</Show>
				<Show when={props.issue.assignees?.length > 0}>
					<span>
						· {t("issueDetail.assignedTo", "assigned to")} {props.issue.assignees.join(", ")}
					</span>
				</Show>
				<Show when={props.issue.milestone}>
					<span>· {props.issue.milestone}</span>
				</Show>
			</div>

			{/* Status pills: state, labels inline, comment count */}
			<div class={s.pills}>
				<span class={cx(s.stateBadge, isOpen() ? s.open : s.closed)}>{isOpen() ? "Open" : "Closed"}</span>
				<For each={props.issue.labels ?? []}>
					{(label) => (
						<span
							class={s.label}
							style={{
								"border-color": label.color ? `#${label.color}` : undefined,
								color: label.color ? `#${label.color}` : undefined,
							}}
						>
							{label.name}
						</span>
					)}
				</For>
				<button
					type="button"
					class={na.commentsPill}
					data-control-layout="row"
					data-testid="issue-comments"
					disabled={!props.issue.url}
					onClick={openConversation}
					title={t("issueDetail.openConversation", "Open the conversation in Tasks")}
				>
					<ToolbarIcons.comment />
					{props.issue.comments_count}
					<Show when={hasNew()}>
						{" · "}
						{t("issueDetail.newCount", "{count} new", { count: String(fresh().count) })}
					</Show>
				</button>
			</div>

			{/* Next action: chosen by issue state */}
			<div class={na.nextAction} data-testid="issue-next-action" data-next-action={nextAction()}>
				<div class={na.nextActionWhy}>
					{nextAction() === "create-pr"
						? t("issueDetail.branchReady", "Branch {branch} ready · no linked PR", {
								branch: autofixBranchName(props.issue.number),
							})
						: nextAction() === "reopen"
							? t("issueDetail.closedWhy", "This issue is closed")
							: hasNew()
								? fresh().count === 1
									? t("issueDetail.newCommentWhy", "1 new comment since the last visit")
									: t("issueDetail.newCommentsWhy", "{count} new comments since the last visit", {
											count: String(fresh().count),
										})
								: t("issueDetail.noBranchWhy", "No branch or worktree for this issue")}
				</div>
				<div class={na.nextActionRow}>
					<Show when={nextAction() === "reopen" && props.onReopen}>
						<Button size="compact" variant="primary" disabled={props.reopening} onClick={() => props.onReopen?.()}>
							{props.reopening ? "..." : t("github.reopen", "Reopen")}
						</Button>
					</Show>
					<Show when={nextAction() === "create-pr"}>
						<Show when={props.onCreatePr}>
							<Button size="compact" variant="primary" disabled={props.creatingPr} onClick={() => props.onCreatePr?.()}>
								<ToolbarIcons.external />
								{props.creatingPr ? "..." : t("issueDetail.createDraftPr", "Create draft PR")}
							</Button>
						</Show>
						<Show when={props.onOpenBranch}>
							<Button size="compact" variant="secondary" onClick={() => props.onOpenBranch?.()}>
								<ToolbarIcons.checkout />
								{t("issueDetail.openWorktree", "Open worktree")}
							</Button>
						</Show>
					</Show>
					<Show when={nextAction() === "work-on-it"}>
						<Show when={replyLeads()}>
							<ReplyBox
								label={t("preview.reply", "Reply")}
								variant="primary"
								icon={<ToolbarIcons.reply />}
								onSend={(body) => preview.act({ kind: "comment", value: body })}
							/>
						</Show>
						<SmartButtonStrip
							placement="issue-popover"
							repoPath={props.repoPath}
							variant={replyLeads() ? "secondary" : "primary"}
							label={t("issueDetail.workOnIt", "Work on it")}
							starts={
								props.onAutofix
									? [
											{
												id: "autofix",
												label: t("issueDetail.autofixInWorktree", "Auto-fix in worktree"),
												onSelect: () => props.onAutofix?.(),
											},
										]
									: []
							}
							defaultPromptId="smart-review-issue"
							workflowSubject={() => ({
								kind: "issue",
								number: props.issue.number,
								title: props.issue.title,
								url: props.issue.url,
							})}
							contextVariables={() => ({
								issue_number: String(props.issue.number),
								issue_title: props.issue.title,
								issue_author: props.issue.author,
								issue_state: props.issue.state,
								issue_url: props.issue.url,
								issue_labels: props.issue.labels?.map((l) => l.name).join(", ") || "none",
								issue_assignees: props.issue.assignees?.join(", ") || "none",
								issue_milestone: props.issue.milestone || "none",
								issue_comments_count: String(props.issue.comments_count),
							})}
						/>
						<Show when={canAssignMe()}>
							<Button size="compact" variant="secondary" disabled={assigning()} onClick={() => void assignMe()}>
								<ToolbarIcons.person />
								{assigning() ? "..." : t("issueDetail.assignMe", "Assign me")}
							</Button>
						</Show>
					</Show>
				</div>
				<Show when={assignError()}>
					<div class={na.nextActionError}>{assignError()}</div>
				</Show>
			</div>

			{props.children}

			{/* Accordion sections: an open issue opens its description, nothing else opens by itself */}
			<PanelSection
				title={t("issueDetail.description", "Description")}
				summary={descriptionSummary(preview)}
				autoOpen={isOpen() && !hasNew()}
				onOpen={preview.load}
			>
				<PreviewDescription preview={preview} />
			</PanelSection>
			<PanelSection
				title={t("issueDetail.comments", "Comments")}
				summary={String(props.issue.comments_count)}
				autoOpen={hasNew()}
				onOpen={preview.load}
			>
				<PreviewComments
					preview={preview}
					filter={(entry) => entry.kind === "normal"}
					empty={t("issueDetail.noComments", "No comments.")}
					onOpenAll={openConversation}
					reply={!replyLeads()}
					newSince={fresh().sinceMs}
				/>
			</PanelSection>
		</>
	);
};
