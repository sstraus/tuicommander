import { Component, Show, For, onMount, onCleanup } from "solid-js";
import { githubStore } from "../../stores/github";
import { repositoriesStore } from "../../stores/repositories";
import { repoSettingsStore } from "../../stores/repoSettings";
import { CiRing } from "../ui/CiRing";
import { relativeTime } from "../../utils/time";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCiIcon, getCiClass } from "../../utils/ciDisplay";

/** Extract "owner/repo" from a GitHub PR URL, e.g. https://github.com/owner/repo/pull/67 */
function extractGithubRepo(url: string): string | null {
  try {
    const parts = new URL(url).pathname.split("/").filter(Boolean);
    if (parts.length >= 2) return `${parts[0]}/${parts[1]}`;
  } catch { /* ignore malformed URL */ }
  return null;
}

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
      <div class="pr-detail-overlay" onClick={props.onClose} />
      <div class={`pr-detail-popover ${props.anchor === "top" ? "pr-detail-anchor-top" : ""}`}>
        <Show when={prData()} fallback={
          <div class="pr-detail-empty">No PR data available for {props.branch}</div>
        }>
          {(pr) => (
            <>
              {/* Repo label: GitHub owner/repo (from PR url) with optional repo color */}
              <div
                class="pr-detail-repo"
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
              <div class="pr-detail-header">
                <span class={`pr-state-badge ${stateClass()}`}>{stateLabel()}</span>
                <span class="pr-detail-title">{pr().title}</span>
                <span
                  class="pr-detail-number pr-detail-link"
                  onClick={() => pr().url && openUrl(pr().url).catch((err) => console.error("Failed to open URL:", err))}
                  title="Open PR on GitHub"
                >
                  #{pr().number}
                </span>
                <button class="pr-detail-close" onClick={props.onClose}>&times;</button>
              </div>

              {/* Merge + review status pills */}
              <Show when={mergeState() || reviewState()}>
                <div class="pr-detail-status-row">
                  <Show when={mergeState()}>
                    {(ms) => (
                      <span class={`merge-state-badge ${ms().cssClass}`}>
                        {ms().label}
                      </span>
                    )}
                  </Show>
                  <Show when={reviewState()}>
                    {(rs) => (
                      <span class={`review-state-badge ${rs().cssClass}`}>
                        {rs().label}
                      </span>
                    )}
                  </Show>
                </div>
              </Show>

              {/* Labels */}
              <Show when={pr().labels?.length > 0}>
                <div class="pr-labels">
                  <For each={pr().labels}>
                    {(label) => (
                      <span
                        class="pr-label"
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
                <div class="pr-merge-direction">
                  <span class="pr-branch-name">{pr().branch}</span>
                  <span class="pr-arrow">{"\u2192"}</span>
                  <span class="pr-branch-name">{pr().base_ref_name}</span>
                </div>
              </Show>

              {/* Timestamps */}
              <Show when={pr().created_at}>
                <div class="pr-timestamps">
                  <span>Created {relativeTime(pr().created_at)}</span>
                  <Show when={pr().updated_at && pr().updated_at !== pr().created_at}>
                    <span class="pr-detail-separator">&middot;</span>
                    <span>Updated {relativeTime(pr().updated_at)}</span>
                  </Show>
                </div>
              </Show>

              {/* Subheader: author + commits */}
              <div class="pr-detail-meta">
                <span class="pr-detail-author">{pr().author}</span>
                <span class="pr-detail-separator">&middot;</span>
                <span>{pr().commits} commit{pr().commits !== 1 ? "s" : ""}</span>
                <span class="pr-detail-separator">&middot;</span>
                <span class="pr-detail-additions">+{pr().additions}</span>
                <span class="pr-detail-deletions">-{pr().deletions}</span>
              </div>

              {/* CI summary */}
              <Show when={checkSummary()?.total ? checkSummary() : null}>
                {(cs) => (
                  <div class="pr-detail-ci-summary">
                    <CiRing
                      passed={cs().passed}
                      failed={cs().failed}
                      pending={cs().pending}
                    />
                    <span class="pr-detail-ci-text">
                      <Show when={cs().failed > 0}>
                        <span class="ci-count failure">{cs().failed} failed</span>
                      </Show>
                      <Show when={cs().pending > 0}>
                        <span class="ci-count pending">{cs().pending} pending</span>
                      </Show>
                      <Show when={cs().passed > 0}>
                        <span class="ci-count success">{cs().passed} passed</span>
                      </Show>
                    </span>
                  </div>
                )}
              </Show>

              {/* Check list */}
              <Show when={checkDetails().length > 0}>
                <div class="pr-detail-checks">
                  <For each={checkDetails()}>
                    {(check) => (
                      <div class="pr-detail-check-item">
                        <span class={`ci-check-icon ${getCiClass(check.state)}`}>
                          {getCiIcon(check.state)}
                        </span>
                        <span class="pr-detail-check-name">{check.context}</span>
                        <span class={`ci-check-status ${getCiClass(check.state)}`}>
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
                  class="pr-detail-open-github"
                  onClick={() => openUrl(pr().url).catch((err) => console.error("Failed to open URL:", err))}
                >
                  Open on GitHub {"\u2197"}
                </div>
              </Show>
            </>
          )}
        </Show>
      </div>
    </>
  );
};
