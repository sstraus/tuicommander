import { Component, Show, For, createSignal, createMemo } from "solid-js";
import { invoke } from "../../invoke";
import { ZoomIndicator, BranchBadge, PrBadge, CiBadge } from "../ui";
import { terminalsStore } from "../../stores/terminals";
import { AGENT_DISPLAY } from "../../agents";
import { BranchPopover } from "../BranchPopover";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { useGitHub } from "../../hooks/useGitHub";
import { githubStore } from "../../stores/github";
import { dictationStore } from "../../stores/dictation";
import { getModifierSymbol } from "../../platform";
import { openUrl } from "@tauri-apps/plugin-opener";

interface CiCheckDetail {
  name: string;
  status: string;
  conclusion: string;
  html_url: string;
}

export interface StatusBarProps {
  fontSize: number;
  defaultFontSize: number;
  statusInfo: string;
  quickSwitcherActive?: boolean;
  onToggleDiff: () => void;
  onToggleMarkdown: () => void;
  onDictationStart: () => void;
  onDictationStop: () => void;
  currentRepoPath?: string;
  cwd?: string;
  onBranchRenamed?: (oldName: string, newName: string) => void;
}

export const StatusBar: Component<StatusBarProps> = (props) => {
  const [showBranchPopover, setShowBranchPopover] = createSignal(false);
  const [showCiPopover, setShowCiPopover] = createSignal(false);
  const [showPrDetailPopover, setShowPrDetailPopover] = createSignal(false);
  const [ciChecks, setCiChecks] = createSignal<CiCheckDetail[]>([]);
  const [ciLoading, setCiLoading] = createSignal(false);
  const [cwdCopied, setCwdCopied] = createSignal(false);

  const handleCopyCwd = async () => {
    if (!props.cwd) return;
    try {
      await navigator.clipboard.writeText(props.cwd);
      setCwdCopied(true);
      setTimeout(() => setCwdCopied(false), 1500);
    } catch (err) {
      console.error("Failed to copy cwd:", err);
    }
  };

  // Shorten path: show ~/ for home, collapse middle segments
  const shortenedCwd = () => {
    const cwd = props.cwd;
    if (!cwd) return null;
    const home = "/Users/";
    let display = cwd;
    if (display.startsWith(home)) {
      const afterHome = display.slice(home.length);
      const slashIdx = afterHome.indexOf("/");
      display = slashIdx >= 0 ? "~" + afterHome.slice(slashIdx) : "~";
    }
    return display;
  };

  // GitHub hook needs a getter function
  const getRepoPath = () => props.currentRepoPath;
  const github = useGitHub(getRepoPath);

  // Get merge state from the centralized github store for the active branch
  const activePrData = createMemo(() => {
    const repoPath = props.currentRepoPath;
    const branch = github.status()?.current_branch;
    if (!repoPath || !branch) return null;
    return githubStore.getBranchPrData(repoPath, branch);
  });

  // Fetch CI check details (Story 060)
  const fetchCiChecks = async () => {
    if (!props.currentRepoPath) return;
    setCiLoading(true);
    try {
      const checks = await invoke<CiCheckDetail[]>("get_ci_checks", { path: props.currentRepoPath });
      setCiChecks(checks);
    } catch (err) {
      console.error("Failed to fetch CI checks:", err);
      setCiChecks([]);
    } finally {
      setCiLoading(false);
    }
  };

  const handleCiBadgeClick = () => {
    setShowCiPopover(true);
    fetchCiChecks();
  };

  const getCiIcon = (conclusion: string) => {
    switch (conclusion) {
      case "success": return "\u2713";
      case "failure": return "\u2717";
      default: return "\u25CF";
    }
  };

  const getCiClass = (conclusion: string) => {
    switch (conclusion) {
      case "success": return "success";
      case "failure": return "failure";
      default: return "pending";
    }
  };


  return (
    <div id="status-bar">
      {/* Left section */}
      <div class="status-section">
        <ZoomIndicator
          fontSize={props.fontSize}
          defaultFontSize={props.defaultFontSize}
        />
        <span id="status-info">{props.statusInfo}</span>
        <Show when={shortenedCwd()}>
          <span
            class="status-cwd"
            title={`Click to copy: ${props.cwd}`}
            onClick={handleCopyCwd}
          >
            {cwdCopied() ? "Copied!" : shortenedCwd()}
          </span>
        </Show>
        <Show when={terminalsStore.getActive()?.agentType}>
          {(agentType) => {
            const display = () => AGENT_DISPLAY[agentType()];
            return (
              <span
                class="status-agent-badge"
                style={{ color: display().color }}
                title={`Agent: ${agentType()}`}
              >
                {display().icon} {agentType()}
              </span>
            );
          }}
        </Show>
      </div>

      {/* GitHub status section */}
      <Show when={github.status()}>
        <div id="github-status" class="status-section">
          <BranchBadge
            branch={github.status()!.current_branch}
            ahead={github.status()!.ahead}
            behind={github.status()!.behind}
            onClick={() => setShowBranchPopover(true)}
          />
          <Show when={github.status()!.pr_status}>
            <PrBadge
              number={github.status()!.pr_status!.number}
              title={github.status()!.pr_status!.title}
              state={github.status()!.pr_status!.state}
              mergeable={activePrData()?.mergeable}
              mergeStateStatus={activePrData()?.merge_state_status}
              onClick={() => setShowPrDetailPopover(true)}
            />
          </Show>
          {/* Show terminal-detected PR if no GitHub API PR and we have terminal detection */}
          <Show when={github.status()!.ci_status}>
            <span onClick={handleCiBadgeClick} style={{ cursor: "pointer" }}>
              <CiBadge
                status={github.status()!.ci_status!.status}
                conclusion={github.status()!.ci_status!.conclusion}
                workflowName={github.status()!.ci_status!.workflow_name}
              />
            </span>
          </Show>
        </div>
      </Show>

      {/* Right section - controls */}
      <div class="status-section status-controls">
        {/* Mic button - hold to talk */}
        <Show when={dictationStore.state.enabled}>
          <button
            id="mic-toggle"
            class="toggle-btn"
            classList={{
              "mic-recording": dictationStore.state.recording,
              "mic-processing": dictationStore.state.processing,
              "mic-loading": dictationStore.state.loading,
            }}
            onMouseDown={(e) => {
              if (e.button === 0) props.onDictationStart();
            }}
            onMouseUp={(e) => {
              if (e.button === 0 && dictationStore.state.recording) props.onDictationStop();
            }}
            onMouseLeave={() => {
              if (dictationStore.state.recording) props.onDictationStop();
            }}
            title={`Voice Dictation (${dictationStore.state.hotkey})`}
            style={{ position: "relative" }}
          >
            <svg class="mic-icon" viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
              <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
              <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
            </svg>
            <span class="hotkey-hint">{dictationStore.state.hotkey}</span>
          </button>
        </Show>

        {/* Toggle buttons */}
        <button id="md-toggle" class="toggle-btn" onClick={props.onToggleMarkdown} title={`Toggle Markdown Panel (${getModifierSymbol()}M)`} style={{ position: "relative" }}>
          MD
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}M</span>
        </button>
        <button id="diff-toggle" class="toggle-btn" onClick={props.onToggleDiff} title={`Toggle Diff Panel (${getModifierSymbol()}D)`} style={{ position: "relative" }}>
          Diff
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}D</span>
        </button>
      </div>

      {/* Branch rename popover */}
      <Show when={showBranchPopover() && github.status()}>
        <BranchPopover
          branch={github.status()!.current_branch}
          repoPath={props.currentRepoPath || null}
          onClose={() => setShowBranchPopover(false)}
          onBranchRenamed={(oldName, newName) => {
            github.refresh();
            props.onBranchRenamed?.(oldName, newName);
          }}
        />
      </Show>

      {/* Rich PR detail popover (Story 093) */}
      <Show when={showPrDetailPopover()}>
        <PrDetailPopover
          repoPath={props.currentRepoPath || ""}
          branch={github.status()?.current_branch || ""}
          onClose={() => setShowPrDetailPopover(false)}
        />
      </Show>

      {/* CI checks popover - fallback (Story 060) */}
      <Show when={showCiPopover()}>
        <div class="ci-popover-overlay" onClick={() => setShowCiPopover(false)} />
        <div class="ci-popover">
          <div class="ci-popover-header">
            <h4>CI Checks</h4>
            <button class="ci-popover-close" onClick={() => setShowCiPopover(false)}>
              &times;
            </button>
          </div>
          <div class="ci-popover-content">
            <Show when={ciLoading()}>
              <div class="ci-popover-empty">Loading checks...</div>
            </Show>
            <Show when={!ciLoading() && ciChecks().length === 0}>
              <div class="ci-popover-empty">No CI checks found</div>
            </Show>
            <Show when={!ciLoading() && ciChecks().length > 0}>
              <For each={ciChecks()}>
                {(check) => (
                  <div
                    class="ci-check-item"
                    onClick={() => check.html_url && openUrl(check.html_url).catch(() => {})}
                  >
                    <span class={`ci-check-icon ${getCiClass(check.conclusion)}`}>
                      {getCiIcon(check.conclusion)}
                    </span>
                    <span class="ci-check-name" title={check.name}>
                      {check.name}
                    </span>
                    <span class={`ci-check-status ${getCiClass(check.conclusion)}`}>
                      {check.conclusion || check.status}
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default StatusBar;
