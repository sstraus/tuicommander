import type { AwaitingInputType } from "../../stores/terminals";

type SoundType = "error" | "question";

/**
 * Edge-detection for notification sounds: returns which sound to play
 * when awaitingInput transitions, or null if no sound should play.
 *
 * Sound fires only on transitions INTO a new state — repeated sets of
 * the same state return null (prevents resize/reflow notification spam).
 * Clearing state (→ null) also returns null (no sound on dismiss).
 */
export function getAwaitingInputSound(
  prev: AwaitingInputType,
  current: AwaitingInputType,
): SoundType | null {
  if (current === prev) return null;
  if (current === "error") return "error";
  if (current === "question") return "question";
  return null;
}
