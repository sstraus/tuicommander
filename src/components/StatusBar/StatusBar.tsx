import { Component, Show, For, createSignal, createMemo, onCleanup } from "solid-js";
import { invoke } from "../../invoke";
import { ZoomIndicator, BranchBadge, PrBadge, CiBadge } from "../ui";
import { terminalsStore } from "../../stores/terminals";
import { AGENT_DISPLAY } from "../../agents";
import { BranchPopover } from "../BranchPopover";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { useGitHub } from "../../hooks/useGitHub";
import { githubStore } from "../../stores/github";
import { rateLimitStore } from "../../stores/ratelimit";
import { formatWaitTime } from "../../rate-limit";
import { dictationStore } from "../../stores/dictation";
import { updaterStore } from "../../stores/updater";
import { getModifierSymbol, shortenHomePath } from "../../platform";
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
  onToggleNotes?: () => void;
  onToggleFileBrowser?: () => void;
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

  // Rate limit countdown — tick every second while any rate limit is active
  const [rlTick, setRlTick] = createSignal(0);
  const rlTimer = setInterval(() => {
    if (rateLimitStore.getRateLimitedCount() > 0) {
      setRlTick((t) => t + 1);
      rateLimitStore.cleanupExpired();
    }
  }, 1000);
  onCleanup(() => clearInterval(rlTimer));

  const rateLimitWarning = createMemo(() => {
    rlTick(); // Subscribe to ticks for reactivity
    const sessions = rateLimitStore.getRateLimitedSessions();
    if (sessions.length === 0) return null;
    // Show the longest remaining wait
    let maxWait = 0;
    for (const sid of sessions) {
      const wait = rateLimitStore.getWaitTime(sid);
      if (wait > maxWait) maxWait = wait;
    }
    return { count: sessions.length, remaining: formatWaitTime(maxWait) };
  });

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
    return shortenHomePath(cwd);
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
    const prData = activePrData();
    if (!props.currentRepoPath || !prData) return;
    setCiLoading(true);
    try {
      const checks = await invoke<CiCheckDetail[]>("get_ci_checks", {
        path: props.currentRepoPath,
        prNumber: prData.number,
      });
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
        <Show when={terminalsStore.getActive()?.usageLimit}>
          {(ul) => (
            <span
              class="status-usage-limit"
              classList={{
                "usage-warning": ul().percentage >= 70 && ul().percentage < 90,
                "usage-critical": ul().percentage >= 90,
              }}
              title={`Claude Code ${ul().limitType} limit: ${ul().percentage}% used`}
            >
              {ul().percentage}% {ul().limitType}
            </span>
          )}
        </Show>
        <Show when={rateLimitWarning()}>
          {(rl) => (
            <span class="status-rate-limit" title={`${rl().count} session(s) rate limited`}>
              ⚠ Rate limited ({rl().remaining})
            </span>
          )}
        </Show>
        <Show when={updaterStore.state.available && !updaterStore.state.downloading}>
          <span
            class="status-update-badge"
            title={`Update to v${updaterStore.state.version}`}
            onClick={() => updaterStore.downloadAndInstall()}
          >
            Update v{updaterStore.state.version}
          </span>
        </Show>
        <Show when={updaterStore.state.downloading}>
          <span class="status-update-badge downloading" title="Downloading update...">
            Updating {updaterStore.state.progress}%
          </span>
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
          <Show when={activePrData()}>
            <PrBadge
              number={activePrData()!.number}
              title={activePrData()!.title}
              state={activePrData()!.state}
              mergeable={activePrData()!.mergeable}
              mergeStateStatus={activePrData()!.merge_state_status}
              onClick={() => setShowPrDetailPopover(true)}
            />
          </Show>
          <Show when={activePrData()?.checks?.total}>
            <span onClick={handleCiBadgeClick} style={{ cursor: "pointer" }}>
              <CiBadge
                status={activePrData()!.checks.failed > 0 ? "completed" : activePrData()!.checks.pending > 0 ? "in_progress" : "completed"}
                conclusion={activePrData()!.checks.failed > 0 ? "failure" : activePrData()!.checks.pending > 0 ? null : "success"}
                workflowName="CI"
              />
            </span>
          </Show>
        </div>
      </Show>

      {/* Right section - controls */}
      <div class="status-section status-controls">
        {/* Toggle buttons */}
        <button id="notes-toggle" class="toggle-btn" onClick={() => props.onToggleNotes?.()} title={`Toggle Ideas Panel (${getModifierSymbol()}N)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" style={{ "vertical-align": "middle" }}><path d="M9 21c0 .5.4 1 1 1h4c.6 0 1-.5 1-1v-1H9v1zm3-19C8.1 2 5 5.1 5 9c0 2.4 1.2 4.5 3 5.7V17c0 .5.4 1 1 1h6c.6 0 1-.5 1-1v-2.3c1.8-1.3 3-3.4 3-5.7 0-3.9-3.1-7-7-7z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}N</span>
        </button>
        <button id="fb-toggle" class="toggle-btn" onClick={() => props.onToggleFileBrowser?.()} title={`File Browser (${getModifierSymbol()}E)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" style={{ "vertical-align": "middle" }}><path d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}E</span>
        </button>
        <button id="md-toggle" class="toggle-btn" onClick={props.onToggleMarkdown} title={`Markdown (${getModifierSymbol()}M)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" style={{ "vertical-align": "middle" }}><path d="M20.56 18H3.44C2.65 18 2 17.37 2 16.59V7.41C2 6.63 2.65 6 3.44 6h17.12c.79 0 1.44.63 1.44 1.41v9.18c0 .78-.65 1.41-1.44 1.41zM6 15.5l2.5-3 1.5 2 2.5-3.5L16 15.5"/><path d="M3.5 13.5V9l2 2.5L7.5 9v4.5" opacity=".7"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}M</span>
        </button>
        <button id="diff-toggle" class="toggle-btn" onClick={props.onToggleDiff} title={`Diff (${getModifierSymbol()}D)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" style={{ "vertical-align": "middle" }}><path d="M9 7H7v2H5v2h2v2h2v-2h2V9H9V7zm7 2h4v2h-4V9zm0 4h4v2h-4v-2zM5 19h14v2H5v-2zM5 3h14v2H5V3z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}D</span>
        </button>

        {/* Mic button - hold to talk (rightmost) */}
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
