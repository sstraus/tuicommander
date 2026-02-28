import { createEffect, on, type Accessor } from "solid-js";
import { notificationManager } from "../notifications";
import type { SessionInfo } from "./useSessions";

const SOUND_KEY = "tuic-mobile-sounds";

/**
 * Watches session state changes and plays notification sounds.
 * Reads the sound-enabled preference from localStorage (shared with SettingsScreen).
 */
export function useMobileNotifications(sessions: Accessor<SessionInfo[]>) {
  const prevStates = new Map<string, { awaiting: boolean; rateLimited: boolean; error: string | undefined; busy: boolean }>();

  createEffect(
    on(sessions, (current) => {
      if (localStorage.getItem(SOUND_KEY) === "false") return;

      for (const s of current) {
        const state = s.state;
        if (!state) continue;

        const prev = prevStates.get(s.session_id);
        const now = {
          awaiting: state.awaiting_input,
          rateLimited: state.rate_limited,
          error: state.last_error,
          busy: state.is_busy,
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
          // Completion: was busy, now idle (no error, no question)
          if (prev.busy && !now.busy && !now.awaiting && !now.error) {
            notificationManager.playCompletion();
          }
        }

        prevStates.set(s.session_id, now);
      }

      // Clean up sessions that disappeared
      for (const id of prevStates.keys()) {
        if (!current.find((s) => s.session_id === id)) {
          prevStates.delete(id);
        }
      }
    }),
  );
}
