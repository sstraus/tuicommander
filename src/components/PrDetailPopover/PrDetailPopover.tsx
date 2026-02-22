import { Component, Show, For, onMount, onCleanup } from "solid-js";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { CiRing } from "../ui/CiRing";
import { relativeTime } from "../../utils/time";
import { handleOpenUrl } from "../../utils/openUrl";
import { getCiIcon, getCiClass } from "../../utils/ciDisplay";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./PrDetailPopover.module.css";

/** Extract "owner/repo" from a GitHub PR URL, e.g. https://github.com/owner/repo/pull/67 */
function extractGithubRepo(url: string): string | null {
  try {
    const parts = new URL(url).pathname.split("/").filter(Boolean);
    if (parts.length >= 2) return `${parts[0]}/${parts[1]}`;
  } catch { /* ignore malformed URL */ }
  return null;
}

/** Map backend PR state strings to CSS module classes */
const STATE_CLASSES: Record<string, string> = {
  open: s.open,
  merged: s.merged,
  closed: s.closed,
  draft: s.draft,
};

/** Map backend merge state CSS class strings to module classes */
const MERGE_STATE_CLASSES: Record<string, string> = {
  clean: s.clean,
  behind: s.behind,
  blocked: s.blocked,
  conflicting: s.conflicting,
};

/** Map backend review state CSS class strings to module classes */
const REVIEW_STATE_CLASSES: Record<string, string> = {
  approved: s.approved,
  "changes-requested": s.changesRequested,
  "review-required": s.reviewRequired,
};

/** Map CI state strings to module classes */
const CI_CLASSES: Record<string, string> = {
  success: s.success,
  failure: s.failure,
  pending: s.pending,
};

export interface PrDetailPopoverProps {
  repoPath: string;
  branch: string;
  onClose: () => void;
  /** Anchor to top-right (toolbar) or bottom-right (status bar, default) */
  anchor?: "top" | "bottom";
}

/** Rich PR detail popover showing PR metadata, diff stats, and CI checks */
export const PrDetailPopover: Component<PrDetailPopoverProps> = (props) => {
  const prData = () => githubStore.getBranchPrData(props.repoPath, props.branch);
  const checkSummary = () => githubStore.getCheckSummary(props.repoPath, props.branch);
  const checkDetails = () => githubStore.getCheckDetails(props.repoPath, props.branch);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onClose();
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  /** Merge readiness label and CSS class — pre-computed by Rust backend */
  const mergeState = () => {
    const label = prData()?.merge_state_label;
    if (!label) return null;
    return { label: label.label, cssClass: label.css_class };
  };

  /** Review decision label — pre-computed by Rust backend */
  const reviewState = () => {
    const label = prData()?.review_state_label;
    if (!label) return null;
    return { label: label.label, cssClass: label.css_class };
  };

  const stateClass = () => {
    if (prData()?.is_draft) return "draft";
    const state = prData()?.state?.toUpperCase();
    switch (state) {
      case "MERGED": return "merged";
      case "CLOSED": return "closed";
      default: return "open";
    }
  };

  const stateLabel = () => {
    if (prData()?.is_draft) return "Draft";
    return prData()?.state || "";
  };

  return (
    <>
      <div class={s.overlay} onClick={props.onClose} />
      <div class={cx(s.popover, props.anchor === "top" && s.anchorTop)}>
        <Show when={prData()} fallback={
          <div class={s.empty}>{t("prDetail.noData", "No PR data available for")} {props.branch}</div>
        }>
          {(pr) => (
            <>
              {/* Repo label: GitHub owner/repo (from PR url) with optional repo color */}
              <div
                class={s.repo}
                style={(() => {
                  const color = repoSettingsStore.get(props.repoPath)?.color
                    || repositoriesStore.getGroupForRepo(props.repoPath)?.color;
                  return color ? { color } : undefined;
                })()}
              >
                {extractGithubRepo(pr().url)
                  ?? repositoriesStore.get(props.repoPath)?.displayName
                  ?? props.repoPath.split("/").pop()}
              </div>

              {/* Header: state badge + title + number */}
              <div class={s.header}>
                <span class={cx(s.stateBadge, STATE_CLASSES[stateClass()])}>{stateLabel()}</span>
                <span class={s.title}>{pr().title}</span>
                <span
                  class={cx(s.number, s.link)}
                  onClick={() => pr().url && handleOpenUrl(pr().url)}
                  title={t("prDetail.openOnGithub", "Open PR on GitHub")}
                >
                  #{pr().number}
                </span>
                <button class={s.close} onClick={props.onClose}>&times;</button>
              </div>

              {/* Merge + review status pills */}
              <Show when={mergeState() || reviewState()}>
                <div class={s.statusRow}>
                  <Show when={mergeState()}>
                    {(ms) => (
                      <span class={cx(s.mergeStateBadge, MERGE_STATE_CLASSES[ms().cssClass])}>
                        {ms().label}
                      </span>
                    )}
                  </Show>
                  <Show when={reviewState()}>
                    {(rs) => (
                      <span class={cx(s.reviewStateBadge, REVIEW_STATE_CLASSES[rs().cssClass])}>
                        {rs().label}
                      </span>
                    )}
                  </Show>
                </div>
              </Show>

              {/* Labels */}
              <Show when={pr().labels?.length > 0}>
                <div class={s.labels}>
                  <For each={pr().labels}>
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

              {/* Merge direction */}
              <Show when={pr().base_ref_name}>
                <div class={s.mergeDirection}>
                  <span class={s.branchName}>{pr().branch}</span>
                  <span class={s.arrow}>{"\u2192"}</span>
                  <span class={s.branchName}>{pr().base_ref_name}</span>
                </div>
              </Show>

              {/* Timestamps */}
              <Show when={pr().created_at}>
                <div class={s.timestamps}>
                  <span>{t("prDetail.created", "Created")} {relativeTime(pr().created_at)}</span>
                  <Show when={pr().updated_at && pr().updated_at !== pr().created_at}>
                    <span class={s.separator}>&middot;</span>
                    <span>{t("prDetail.updated", "Updated")} {relativeTime(pr().updated_at)}</span>
                  </Show>
                </div>
              </Show>

              {/* Subheader: author + commits */}
              <div class={s.meta}>
                <span class={s.author}>{pr().author}</span>
                <span class={s.separator}>&middot;</span>
                <span>{pr().commits} commit{pr().commits !== 1 ? "s" : ""}</span>
                <span class={s.separator}>&middot;</span>
                <span class={s.additions}>+{pr().additions}</span>
                <span class={s.deletions}>-{pr().deletions}</span>
              </div>

              {/* CI summary */}
              <Show when={checkSummary()?.total ? checkSummary() : null}>
                {(cs) => (
                  <div class={s.ciSummary}>
                    <CiRing
                      passed={cs().passed}
                      failed={cs().failed}
                      pending={cs().pending}
                    />
                    <span class={s.ciText}>
                      <Show when={cs().failed > 0}>
                        <span class={cx(s.ciCount, s.failure)}>{cs().failed} {t("prDetail.failed", "failed")}</span>
                      </Show>
                      <Show when={cs().pending > 0}>
                        <span class={cx(s.ciCount, s.pending)}>{cs().pending} {t("prDetail.pending", "pending")}</span>
                      </Show>
                      <Show when={cs().passed > 0}>
                        <span class={cx(s.ciCount, s.success)}>{cs().passed} {t("prDetail.passed", "passed")}</span>
                      </Show>
                    </span>
                  </div>
                )}
              </Show>

              {/* Check list */}
              <Show when={checkDetails().length > 0}>
                <div class={s.checks}>
                  <For each={checkDetails()}>
                    {(check) => (
                      <div class={s.checkItem}>
                        <span class={cx(s.checkIcon, CI_CLASSES[getCiClass(check.state)])}>
                          {getCiIcon(check.state)}
                        </span>
                        <span class={s.checkName}>{check.context}</span>
                        <span class={cx(s.checkStatus, CI_CLASSES[getCiClass(check.state)])}>
                          {check.state}
                        </span>
                      </div>
                    )}
                  </For>
                </div>
              </Show>

              {/* Open on GitHub */}
              <Show when={pr().url}>
                <div
                  class={s.openGithub}
                  onClick={() => handleOpenUrl(pr().url)}
                >
                  {t("prDetail.openOnGithub", "Open on GitHub")} {"\u2197"}
                </div>
              </Show>
            </>
          )}
        </Show>
      </div>
    </>
  );
};
