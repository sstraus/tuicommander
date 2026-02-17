/**
 * Shared timestamp for deduplicating native menu accelerators and DOM keydown handlers.
 * When a menu-action event fires, we set this timestamp. The DOM keydown handler
 * checks it and skips if a menu event fired within 200ms.
 *
 * Stored in a separate module to avoid circular dependencies between App.tsx
 * and useKeyboardShortcuts.ts.
 */
export let lastMenuActionTime = 0;

export function setLastMenuActionTime(time: number): void {
  lastMenuActionTime = time;
}
