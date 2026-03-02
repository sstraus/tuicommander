import { Component, Show, createMemo, onMount, onCleanup } from "solid-js";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { handleOpenUrl } from "../../utils/openUrl";
import { t } from "../../i18n";
import { cx } from "../../utils";
import { PrDetailContent } from "./PrDetailContent";
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

  const repoColor = createMemo(() =>
    repoSettingsStore.get(props.repoPath)?.color
      || repositoriesStore.getGroupForRepo(props.repoPath)?.color
      || undefined,
  );

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
                style={repoColor() ? { color: repoColor() } : undefined}
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

              {/* Shared body content: status pills, labels, meta, CI, checks, open link */}
              <PrDetailContent repoPath={props.repoPath} branch={props.branch} />
            </>
          )}
        </Show>
      </div>
    </>
  );
};
