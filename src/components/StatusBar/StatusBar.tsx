import { Component, Show, createSignal, createMemo, onCleanup, onMount } from "solid-js";
import { ZoomIndicator, BranchBadge, PrBadge, CiBadge } from "../ui";
import { terminalsStore } from "../../stores/terminals";
import { AGENT_DISPLAY } from "../../agents";
import { AgentIcon } from "../ui/AgentIcon";
import { BranchPopover } from "../BranchPopover";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { useGitHub } from "../../hooks/useGitHub";
import { githubStore } from "../../stores/github";
import { rateLimitStore } from "../../stores/ratelimit";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { formatWaitTime } from "../../rate-limit";
import { dictationStore } from "../../stores/dictation";
import { updaterStore } from "../../stores/updater";
import { getModifierSymbol, shortenHomePath } from "../../platform";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./StatusBar.module.css";

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
  const [showPrDetailPopover, setShowPrDetailPopover] = createSignal(false);
  const [cwdCopied, setCwdCopied] = createSignal(false);

  // Rate limit countdown — tick every second while any rate limit is active
  const [rlTick, setRlTick] = createSignal(0);
  onMount(() => {
    const rlTimer = setInterval(() => {
      if (rateLimitStore.getRateLimitedCount() > 0) {
        setRlTick((t) => t + 1);
        rateLimitStore.cleanupExpired();
      }
    }, 1000);
    onCleanup(() => clearInterval(rlTimer));
  });

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

  const handleCiBadgeClick = () => {
    setShowPrDetailPopover(true);
  };

  return (
    <div id="status-bar" class={s.bar}>
      {/* Left section */}
      <div class={s.section}>
        <ZoomIndicator
          fontSize={props.fontSize}
          defaultFontSize={props.defaultFontSize}
        />
        <span class={s.info}>{props.statusInfo}</span>
        <Show when={shortenedCwd()}>
          <span
            class={s.cwd}
            title={`${t("statusBar.clickCopy", "Click to copy:")} ${props.cwd}`}
            onClick={handleCopyCwd}
          >
            {cwdCopied() ? t("statusBar.copied", "Copied!") : shortenedCwd()}
          </span>
        </Show>
        <Show when={terminalsStore.getActive()?.agentType}>
          {(agentType) => {
            const display = () => AGENT_DISPLAY[agentType()];
            return (
              <span
                class={s.agentBadge}
                style={{ color: display().color }}
                title={`${t("statusBar.agent", "Agent:")} ${agentType()}`}
              >
                <AgentIcon agent={agentType()} size={12} /> {agentType()}
              </span>
            );
          }}
        </Show>
        <Show when={terminalsStore.getActive()?.usageLimit}>
          {(ul) => (
            <span
              class={cx(
                s.usageLimit,
                ul().percentage >= 90 && s.usageCritical,
                ul().percentage >= 70 && ul().percentage < 90 && s.usageWarning,
              )}
              title={`Claude Code ${ul().limitType} limit: ${ul().percentage}% used`}
            >
              {ul().percentage}% {ul().limitType}
            </span>
          )}
        </Show>
        <Show when={statusBarTicker.getCurrentMessage()}>
          {(msg) => (
            <span
              class={cx(s.tickerMessage, msg().priority >= 80 && s.tickerWarning)}
              title={msg().text}
            >
              <Show when={msg().icon}>
                {(icon) => <span class={s.tickerIcon} innerHTML={icon()} />}
              </Show>
              {msg().text}
            </span>
          )}
        </Show>
        <Show when={rateLimitWarning()}>
          {(rl) => (
            <span class={s.rateLimit} title={`${rl().count} session(s) rate limited`}>
              ⚠ {t("statusBar.rateLimited", "Rate limited")} ({rl().remaining})
            </span>
          )}
        </Show>
        <Show when={updaterStore.state.available && !updaterStore.state.downloading}>
          <span
            class={s.updateBadge}
            title={`${t("statusBar.updateTo", "Update to")} v${updaterStore.state.version}`}
            onClick={() => updaterStore.downloadAndInstall()}
          >
            {t("statusBar.update", "Update")} v{updaterStore.state.version}
          </span>
        </Show>
        <Show when={updaterStore.state.downloading}>
          <span class={cx(s.updateBadge, s.downloading)} title={t("statusBar.downloading", "Downloading update...")}>
            {t("statusBar.updating", "Updating")} {updaterStore.state.progress}%
          </span>
        </Show>
      </div>

      {/* GitHub status section */}
      <Show when={github.status()}>
        {(gs) => (
          <div class={s.githubStatus}>
            <BranchBadge
              branch={gs().current_branch}
              ahead={gs().ahead}
              behind={gs().behind}
              onClick={() => setShowBranchPopover(true)}
            />
            <Show when={activePrData()}>
              {(pr) => (
                <PrBadge
                  number={pr().number}
                  title={pr().title}
                  state={pr().state}
                  mergeable={pr().mergeable}
                  mergeStateStatus={pr().merge_state_status}
                  onClick={() => setShowPrDetailPopover(true)}
                />
              )}
            </Show>
            <Show when={activePrData()?.checks}>
              {(checks) => (
                <Show when={checks().total > 0}>
                  <span onClick={handleCiBadgeClick} style={{ cursor: "pointer" }}>
                    <CiBadge
                      status={checks().failed > 0 ? "completed" : checks().pending > 0 ? "in_progress" : "completed"}
                      conclusion={checks().failed > 0 ? "failure" : checks().pending > 0 ? null : "success"}
                      workflowName="CI"
                    />
                  </span>
                </Show>
              )}
            </Show>
          </div>
        )}
      </Show>

      {/* Right section - controls */}
      <div class={cx(s.section, s.controls)}>
        {/* Toggle buttons */}
        <button class="toggle-btn" onClick={() => props.onToggleNotes?.()} title={`${t("statusBar.toggleNotes", "Toggle Ideas Panel")} (${getModifierSymbol()}N)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M9 21c0 .5.4 1 1 1h4c.6 0 1-.5 1-1v-1H9v1zm3-19C8.1 2 5 5.1 5 9c0 2.4 1.2 4.5 3 5.7V17c0 .5.4 1 1 1h6c.6 0 1-.5 1-1v-2.3c1.8-1.3 3-3.4 3-5.7 0-3.9-3.1-7-7-7z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}N</span>
        </button>
        <button class="toggle-btn" onClick={() => props.onToggleFileBrowser?.()} title={`${t("statusBar.fileBrowser", "File Browser")} (${getModifierSymbol()}E)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}E</span>
        </button>
        <button class="toggle-btn" onClick={props.onToggleMarkdown} title={`${t("statusBar.markdown", "Markdown")} (${getModifierSymbol()}M)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 208 128" width="16" height="10" fill="currentColor"><rect x="5" y="5" width="198" height="118" rx="12" fill="none" stroke="currentColor" stroke-width="12"/><path d="M30 98V30h20l20 25 20-25h20v68h-20V59L70 84 50 59v39H30zm125 0l-30-33h20V30h20v35h20l-30 33z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}M</span>
        </button>
        <button class="toggle-btn" onClick={props.onToggleDiff} title={`${t("statusBar.diff", "Diff")} (${getModifierSymbol()}D)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M9 7H7v2H5v2h2v2h2v-2h2V9H9V7zm7 2h4v2h-4V9zm0 4h4v2h-4v-2zM5 19h14v2H5v-2zM5 3h14v2H5V3z"/></svg>
          <span class={`hotkey-hint ${props.quickSwitcherActive ? "quick-switcher-active" : ""}`}>{getModifierSymbol()}D</span>
        </button>

        {/* Mic button - hold to talk (rightmost) */}
        <Show when={dictationStore.state.enabled}>
          <button
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
            title={`${t("statusBar.voiceDictation", "Voice Dictation")} (${dictationStore.state.hotkey})`}
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
      <Show when={showBranchPopover() ? github.status() : null}>
        {(gs) => (
          <BranchPopover
            branch={gs().current_branch}
            repoPath={props.currentRepoPath || null}
            onClose={() => setShowBranchPopover(false)}
            onBranchRenamed={(oldName, newName) => {
              github.refresh();
              props.onBranchRenamed?.(oldName, newName);
            }}
          />
        )}
      </Show>

      {/* Rich PR detail popover */}
      <Show when={showPrDetailPopover()}>
        <PrDetailPopover
          repoPath={props.currentRepoPath || ""}
          branch={github.status()?.current_branch || ""}
          onClose={() => setShowPrDetailPopover(false)}
        />
      </Show>

    </div>
  );
};

export default StatusBar;
