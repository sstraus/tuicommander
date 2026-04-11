import { Component, Show, For, type JSX } from "solid-js";
import { relativeTime } from "../../utils/time";
import { t } from "../../i18n";
import { cx } from "../../utils";
import type { GitHubIssue } from "../../types";
import s from "./IssueDetailContent.module.css";

export interface IssueDetailContentProps {
  issue: GitHubIssue;
  repoPath: string;
  /** Extra content rendered after metadata (e.g. action buttons) */
  children?: JSX.Element;
}

/** Shared issue detail body: state badge, labels, assignees, milestone, timestamps,
 *  children slot for action buttons, smart prompts, and Open on GitHub link.
 *  Mirrors the PrDetailContent pattern for layout consistency. */
export const IssueDetailContent: Component<IssueDetailContentProps> = (props) => {
  const isOpen = () => props.issue.state?.toUpperCase() === "OPEN";

  return (
    <>
      {/* State badge + author + comments */}
      <div class={s.meta}>
        <span class={cx(s.stateBadge, isOpen() ? s.open : s.closed)}>
          {isOpen() ? "Open" : "Closed"}
        </span>
        <span class={s.separator}>&middot;</span>
        <span class={s.author}>{props.issue.author}</span>
        <Show when={props.issue.comments_count > 0}>
          <span class={s.separator}>&middot;</span>
          <span>{props.issue.comments_count} comment{props.issue.comments_count !== 1 ? "s" : ""}</span>
        </Show>
      </div>

      {/* Labels */}
      <Show when={props.issue.labels?.length > 0}>
        <div class={s.labels}>
          <For each={props.issue.labels}>
            {(label) => (
              <span
                class={s.label}
                style={{
                  "background-color": label.background_color || undefined,
                  "border-color": label.color ? `#${label.color}` : undefined,
                  color: label.text_color || undefined,
                }}
              >
                {label.name}
              </span>
            )}
          </For>
        </div>
      </Show>

      {/* Assignees */}
      <Show when={props.issue.assignees?.length > 0}>
        <div class={s.assignees}>
          <For each={props.issue.assignees}>
            {(assignee) => <span class={s.assignee}>{assignee}</span>}
          </For>
        </div>
      </Show>

      {/* Milestone */}
      <Show when={props.issue.milestone}>
        <div class={s.milestone}>
          <svg class={s.milestoneIcon} width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 0a8 8 0 108 8A8 8 0 008 0zm0 14.5A6.5 6.5 0 1114.5 8 6.508 6.508 0 018 14.5zM8 4a4 4 0 104 4 4 4 0 00-4-4zm0 6a2 2 0 112-2 2 2 0 01-2 2z"/></svg>
          <span>{props.issue.milestone}</span>
        </div>
      </Show>

      {/* Timestamps */}
      <Show when={props.issue.created_at}>
        <div class={s.timestamps}>
          <span>{t("prDetail.created", "Created")} {relativeTime(props.issue.created_at)}</span>
          <Show when={props.issue.updated_at && props.issue.updated_at !== props.issue.created_at}>
            <span class={s.separator}>&middot;</span>
            <span>{t("prDetail.updated", "Updated")} {relativeTime(props.issue.updated_at)}</span>
          </Show>
        </div>
      </Show>

      {/* Extra content (action buttons, smart prompts, open link) */}
      {props.children}
    </>
  );
};
