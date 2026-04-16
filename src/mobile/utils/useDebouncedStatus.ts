import { createSignal, createEffect, onCleanup } from "solid-js";
import type { SessionInfo } from "../useSessions";
import type { SessionStatus } from "../components/StatusBadge";
import { deriveStatus } from "./deriveStatus";

/** How long "busy" holds after raw status leaves busy.
 * Matches desktop (2s) — SSE push for shell-state makes real-time updates
 * available without waiting for the 3s poll cycle. */
const BUSY_HOLD_MS = 2000;

/**
 * Wraps deriveStatus with a busy-hold debounce: once a session enters "busy",
 * it stays "busy" for BUSY_HOLD_MS after the raw status leaves "busy".
 * High-priority states (error, rate-limited, question) override the hold.
 */
export function useDebouncedStatus(session: () => SessionInfo): () => SessionStatus {
  const initial = deriveStatus(session());
  const [debounced, setDebounced] = createSignal<SessionStatus>(initial);
  let cooldownTimer: ReturnType<typeof setTimeout> | undefined;
  let inBusyHold = initial === "busy";

  createEffect(() => {
    const raw = deriveStatus(session());

    if (raw === "busy") {
      if (cooldownTimer != null) {
        clearTimeout(cooldownTimer);
        cooldownTimer = undefined;
      }
      inBusyHold = true;
      setDebounced("busy");
    } else if (inBusyHold) {
      // Higher-priority states override the busy hold immediately
      if (raw === "rate-limited" || raw === "error" || raw === "question") {
        if (cooldownTimer != null) {
          clearTimeout(cooldownTimer);
          cooldownTimer = undefined;
        }
        inBusyHold = false;
        setDebounced(raw);
      } else if (cooldownTimer == null) {
        // Start cooldown — keep showing busy until timer fires
        cooldownTimer = setTimeout(() => {
          cooldownTimer = undefined;
          inBusyHold = false;
          // Re-derive at cooldown end to get the latest state
          setDebounced(deriveStatus(session()));
        }, BUSY_HOLD_MS);
      }
    } else {
      setDebounced(raw);
    }
  });

  onCleanup(() => {
    if (cooldownTimer != null) clearTimeout(cooldownTimer);
  });

  return debounced;
}
