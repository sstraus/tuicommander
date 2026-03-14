import { createEffect, on, onCleanup, type Accessor } from "solid-js";
import { notificationManager } from "../notifications";
import type { SessionInfo } from "./useSessions";
import { getMobileCompletionAction } from "./utils/mobileCompletionDecision";

const SOUND_KEY = "tuic-mobile-sounds";

interface PrevState {
  awaiting: boolean;
  rateLimited: boolean;
  error: string | undefined;
  busy: boolean;
  busySince: number | null;
}

/**
 * Watches session state changes and plays notification sounds.
 * Reads the sound-enabled preference from localStorage (shared with SettingsScreen).
 *
 * Completion notifications mirror the desktop logic: require >=5s busy duration,
 * suppress when sub-tasks are active, and defer 10s for agent sessions.
 */
export function useMobileNotifications(sessions: Accessor<SessionInfo[]>) {
  const prevStates = new Map<string, PrevState>();
  const deferredTimers = new Map<string, ReturnType<typeof setTimeout>>();

  onCleanup(() => {
    for (const timer of deferredTimers.values()) clearTimeout(timer);
    deferredTimers.clear();
  });

  createEffect(
    on(sessions, (current) => {
      if (localStorage.getItem(SOUND_KEY) === "false") return;

      for (const s of current) {
        const state = s.state;
        if (!state) continue;

        const prev = prevStates.get(s.session_id);
        const nowBusy = state.shell_state === "busy";
        const now: PrevState = {
          awaiting: state.awaiting_input,
          rateLimited: state.rate_limited,
          error: state.last_error,
          busy: nowBusy,
          busySince: nowBusy
            ? (prev?.busySince ?? Date.now())
            : null,
        };

        if (prev) {
          // Question: wasn't awaiting, now is
          if (!prev.awaiting && now.awaiting) {
            notificationManager.playQuestion();
          }
          // Rate limited: wasn't, now is
          if (!prev.rateLimited && now.rateLimited) {
            notificationManager.playWarning();
          }
          // Error: no error before, has error now
          if (!prev.error && now.error) {
            notificationManager.playError();
          }
          // Completion: was busy, now idle — run through decision logic
          if (prev.busy && !now.busy) {
            // Cancel any pending deferred completion for this session
            const existing = deferredTimers.get(s.session_id);
            if (existing) {
              clearTimeout(existing);
              deferredTimers.delete(s.session_id);
            }

            const busyDurationMs = prev.busySince
              ? Date.now() - prev.busySince
              : 0;

            const decision = getMobileCompletionAction({
              busyDurationMs,
              activeSubTasks: state.active_sub_tasks ?? 0,
              awaiting: now.awaiting,
              error: !!now.error,
              isAgent: !!state.agent_type,
            });

            if (decision.action === "fire") {
              notificationManager.playCompletion();
            } else if (decision.action === "defer") {
              deferredTimers.set(
                s.session_id,
                setTimeout(() => {
                  deferredTimers.delete(s.session_id);
                  notificationManager.playCompletion();
                }, decision.delayMs),
              );
            }
          }
        }

        prevStates.set(s.session_id, now);
      }

      // Clean up sessions that disappeared — cancel deferred timers too
      for (const id of prevStates.keys()) {
        if (!current.find((s) => s.session_id === id)) {
          prevStates.delete(id);
          const timer = deferredTimers.get(id);
          if (timer) {
            clearTimeout(timer);
            deferredTimers.delete(id);
          }
        }
      }
    }),
  );
}
