import { Component, Show, createSignal, createMemo, createEffect, onCleanup, onMount } from "solid-js";
import { ZoomIndicator, PrBadge, CiBadge } from "../ui";
import { terminalsStore } from "../../stores/terminals";
import { AGENT_DISPLAY } from "../../agents";
import { AgentIcon } from "../ui/AgentIcon";
import { PrDetailPopover } from "../PrDetailPopover/PrDetailPopover";
import { useGitHub } from "../../hooks/useGitHub";
import { rateLimitStore } from "../../stores/ratelimit";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { TickerArea } from "./TickerArea";
import { formatWaitTime } from "../../rate-limit";
import { dictationStore } from "../../stores/dictation";
import { notesStore } from "../../stores/notes";
import { activePrStatus } from "../../utils/mergedPrGrace";
import { getModifierSymbol, shortenHomePath } from "../../platform";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import s from "./StatusBar.module.css";

export interface StatusBarProps {
  fontSize: number;
  defaultFontSize: number;
  statusInfo: string;
  onToggleDiff: () => void;
  onToggleMarkdown: () => void;
  onToggleNotes?: () => void;
  onToggleFileBrowser?: () => void;
  onToggleErrorLog?: () => void;
  onDictationStart: () => void;
  onDictationStop: () => void;
  currentRepoPath?: string;
  cwd?: string;
  onBranchRenamed?: (oldName: string, newName: string) => void;
}

export const StatusBar: Component<StatusBarProps> = (props) => {
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
      appLogger.error("app", "Failed to copy cwd", err);
    }
  };

  // Shorten path: show ~/ for home, collapse middle segments
  const shortenedCwd = () => {
    const cwd = props.cwd;
    if (!cwd) return null;
    return shortenHomePath(cwd);
  };

  // Pendulum ticker: detect overflow on notification text
  let infoContainerRef: HTMLSpanElement | undefined;
  let infoTextRef: HTMLSpanElement | undefined;
  const [tickerActive, setTickerActive] = createSignal(false);
  const [infoDismissed, setInfoDismissed] = createSignal(false);
  const [dismissedText, setDismissedText] = createSignal("");

  createEffect(() => {
    // Subscribe to statusInfo changes to re-measure overflow
    void props.statusInfo;
    // Defer measurement to after DOM update
    const rafId = requestAnimationFrame(() => {
      if (!infoContainerRef || !infoTextRef) return;
      const overflowPx = infoTextRef.scrollWidth - infoContainerRef.clientWidth;
      if (overflowPx > 0) {
        // ~50px/s reading speed, minimum 4s cycle
        const duration = Math.max(4, (overflowPx / 50) * 2 + 4);
        infoContainerRef.style.setProperty("--overflow-px", String(overflowPx));
        infoContainerRef.style.setProperty("--ticker-duration", `${duration}s`);
        setTickerActive(true);
      } else {
        setTickerActive(false);
      }
    });
    onCleanup(() => cancelAnimationFrame(rafId));
  });

  // GitHub hook needs a getter function
  const getRepoPath = () => props.currentRepoPath;
  const github = useGitHub(getRepoPath);

  const notesBadgeCount = () => notesStore.filteredCount(props.currentRepoPath ?? null);

  // PR data with lifecycle rules (CLOSED: hidden, MERGED: grace period, OPEN: shown).
  // Re-evaluates every second so the merged grace period ticks down.
  const [prTick, setPrTick] = createSignal(0);

  onMount(() => {
    const prTimer = setInterval(() => setPrTick((t) => t + 1), 1000);
    onCleanup(() => clearInterval(prTimer));
  });

  const activePrData = createMemo(() => {
    prTick(); // subscribe to 1s tick for merged PR countdown
    const repoPath = props.currentRepoPath;
    const branch = github.status()?.current_branch;
    if (!repoPath || !branch) return null;
    return activePrStatus(repoPath, branch);
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
        <Show when={!infoDismissed() || props.statusInfo !== dismissedText()}>
          <span
            class={s.info}
            ref={infoContainerRef}
            onClick={() => { setInfoDismissed(true); setDismissedText(props.statusInfo); }}
            style={{ cursor: tickerActive() ? "pointer" : undefined }}
            title={tickerActive() ? props.statusInfo : undefined}
          >
            <span
              class={cx(s.infoTicker, tickerActive() && s.infoTickerActive)}
              ref={infoTextRef}
            >
              {props.statusInfo}
            </span>
          </span>
        </Show>
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
            const ul = () => terminalsStore.getActive()?.usageLimit ?? null;
            const rl = rateLimitWarning;
            // When agent is claude, absorb the claude-usage ticker into the badge
            const claudeTicker = () =>
              agentType() === "claude"
                ? statusBarTicker.getAll().find((m) => m.pluginId === "claude-usage")
                : null;
            return (
              <span
                class={cx(s.agentBadge, claudeTicker()?.onClick && s.tickerClickable)}
                onClick={() => claudeTicker()?.onClick?.()}
                title={
                  rl()
                    ? `${agentType()} — ${rl()!.count} session(s) rate limited (${rl()!.remaining})`
                    : claudeTicker()
                      ? claudeTicker()!.text
                      : ul()
                        ? `${agentType()} — ${ul()!.percentage}% of ${ul()!.limitType} limit used`
                        : `${t("statusBar.agent", "Agent:")} ${agentType()}`
                }
              >
                <span style={{ color: display().color }}><AgentIcon agent={agentType()} size={12} /></span>
                {rl()
                  ? <span class={s.agentRateLimited}> ⚠ {rl()!.remaining}</span>
                  : claudeTicker()
                    ? <span class={cx(
                        s.agentUsage,
                        claudeTicker()!.priority >= 90 && s.agentUsageCritical,
                        claudeTicker()!.priority >= 50 && claudeTicker()!.priority < 90 && s.agentUsageWarning,
                      )}> {claudeTicker()!.text.replace(/^Claude:\s*/, "")}</span>
                    : ul()
                      ? <span class={cx(
                          s.agentUsage,
                          ul()!.percentage >= 90 && s.agentUsageCritical,
                          ul()!.percentage >= 70 && ul()!.percentage < 90 && s.agentUsageWarning,
                        )}> {ul()!.percentage}% {ul()!.limitType}</span>
                      : <span style={{ color: display().color }}> {agentType()}</span>
                }
              </span>
            );
          }}
        </Show>
        <TickerArea />
      </div>

      {/* GitHub PR + CI badges */}
      <Show when={activePrData()}>
        <div class={s.githubStatus}>
          <PrBadge
            number={activePrData()!.number}
            title={activePrData()!.title}
            state={activePrData()!.state}
            mergeable={activePrData()!.mergeable}
            mergeStateStatus={activePrData()!.merge_state_status}
            onClick={() => setShowPrDetailPopover(true)}
          />
          <Show when={activePrData()!.checks && activePrData()!.checks!.total > 0}>
            <span onClick={handleCiBadgeClick} style={{ cursor: "pointer" }}>
              <CiBadge
                status={activePrData()!.checks!.failed > 0 ? "completed" : activePrData()!.checks!.pending > 0 ? "in_progress" : "completed"}
                conclusion={activePrData()!.checks!.failed > 0 ? "failure" : activePrData()!.checks!.pending > 0 ? null : "success"}
                workflowName="CI"
              />
            </span>
          </Show>
        </div>
      </Show>

      {/* Right section - controls */}
      <div class={cx(s.section, s.controls)}>
        {/* Toggle buttons */}
        <Show when={appLogger.unseenErrorCount() > 0}>
          <button class="toggle-btn" onClick={() => props.onToggleErrorLog?.()} title={`Error Log (${getModifierSymbol()}${"\u21e7"}E)`} style={{ position: "relative" }}>
            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/></svg>
            <span class={s.toggleBadge} style={{ background: "var(--error)", color: "#000" }}>{appLogger.unseenErrorCount()}</span>
          </button>
        </Show>
        <button class="toggle-btn" onClick={() => props.onToggleNotes?.()} title={`${t("statusBar.toggleNotes", "Toggle Ideas Panel")} (${getModifierSymbol()}N)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M9 21c0 .5.4 1 1 1h4c.6 0 1-.5 1-1v-1H9v1zm3-19C8.1 2 5 5.1 5 9c0 2.4 1.2 4.5 3 5.7V17c0 .5.4 1 1 1h6c.6 0 1-.5 1-1v-2.3c1.8-1.3 3-3.4 3-5.7 0-3.9-3.1-7-7-7z"/></svg>
          <Show when={notesBadgeCount() > 0}>
            <span class={s.toggleBadge}>{notesBadgeCount()}</span>
          </Show>
        </button>
        <button class="toggle-btn" onClick={() => props.onToggleFileBrowser?.()} title={`${t("statusBar.fileBrowser", "File Browser")} (${getModifierSymbol()}E)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/></svg>
        </button>
        <button class="toggle-btn" onClick={props.onToggleMarkdown} title={`${t("statusBar.markdown", "Markdown")} (${getModifierSymbol()}M)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 208 128" width="16" height="10" fill="currentColor"><rect x="5" y="5" width="198" height="118" rx="12" fill="none" stroke="currentColor" stroke-width="12"/><path d="M30 98V30h20l20 25 20-25h20v68h-20V59L70 84 50 59v39H30zm125 0l-30-33h20V30h20v35h20l-30 33z"/></svg>
        </button>
        <button class="toggle-btn" onClick={props.onToggleDiff} title={`${t("statusBar.diff", "Diff")} (${getModifierSymbol()}D)`} style={{ position: "relative" }}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M9 7H7v2H5v2h2v2h2v-2h2V9H9V7zm7 2h4v2h-4V9zm0 4h4v2h-4v-2zM5 19h14v2H5v-2zM5 3h14v2H5V3z"/></svg>
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
              if (e.button === 0) props.onDictationStop();
            }}
            onMouseLeave={() => {
              if (dictationStore.state.recording || dictationStore.state.loading) props.onDictationStop();
            }}
            title={`${t("statusBar.voiceDictation", "Voice Dictation")} (${dictationStore.state.hotkey})`}
            style={{ position: "relative" }}
          >
            <svg class="mic-icon" viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
              <path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3z"/>
              <path d="M17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/>
            </svg>
          </button>
        </Show>
      </div>

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
