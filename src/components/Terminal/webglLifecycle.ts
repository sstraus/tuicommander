import type { Terminal } from "@xterm/xterm";
import type { WebglAddon } from "@xterm/addon-webgl";

export const ATLAS_BASE_FONT = 14;
export const ATLAS_BASE_MIN_PAGES = 3;
export const ATLAS_BASE_MIN_INTERVAL_MS = 30_000;

/** Manages the WebglAddon lifecycle: creation, context-loss recovery,
 *  and adaptive atlas stress detection with full renderer rebuilds.
 *
 *  Extracted from Terminal.tsx to isolate the WebGL workarounds into a
 *  testable, replaceable module. */
export class WebglLifecycle {
  addon: WebglAddon | undefined;
  /** Current effective page threshold — rebuild triggers after this many atlas pages. */
  minPages = ATLAS_BASE_MIN_PAGES;
  /** Current effective cooldown between rebuilds (ms). */
  minIntervalMs = ATLAS_BASE_MIN_INTERVAL_MS;
  /** Optional callback invoked when a full atlas rebuild is needed.
   *  The host (Terminal.tsx) wires this to schedule the actual rebuild
   *  via queueMicrotask so dispose doesn't run inside an addon callback. */
  onRebuild: (() => void) | undefined;

  private factory: () => WebglAddon;
  private terminal: Terminal | undefined;
  private pagesSinceCleanup = 0;
  private lastCleanupMs = 0;

  constructor(factory: () => WebglAddon) {
    this.factory = factory;
  }

  /** Recalculate atlas cleanup thresholds based on current font size.
   *  At 2× the base font, pages hold ¼ as many glyphs → threshold drops
   *  and cooldown shrinks proportionally. */
  updateThresholds(fontSize: number): void {
    const ratio = Math.max(1, fontSize / ATLAS_BASE_FONT);
    this.minPages = Math.max(1, Math.round(ATLAS_BASE_MIN_PAGES / ratio));
    this.minIntervalMs = Math.max(5_000, Math.round(ATLAS_BASE_MIN_INTERVAL_MS / ratio));
  }

  /** Create the WebglAddon, wire lifecycle events, and load it into the terminal. */
  attach(terminal: Terminal): void {
    this.terminal = terminal;
    this.addon = this.createAddon();
  }

  /** Dispose the current addon and clear state. */
  dispose(): void {
    if (this.addon) {
      try {
        this.addon.dispose();
      } catch {
        // Addon may already be disposed (e.g. context loss race) — ignore.
      }
      this.addon = undefined;
    }
  }

  /** Called by the host after a rebuild completes (new addon created). */
  addonReplaced(newAddon: WebglAddon | undefined): void {
    this.addon = newAddon;
  }

  private createAddon(): WebglAddon | undefined {
    if (!this.terminal) return undefined;
    try {
      const addon = this.factory();
      addon.onContextLoss(() => {
        addon.dispose();
        if (this.addon === addon) this.addon = undefined;
        this.onRebuild?.();
      });
      addon.onAddTextureAtlasCanvas(() => {
        this.pagesSinceCleanup++;
        const now = performance.now();
        if (
          this.pagesSinceCleanup >= this.minPages &&
          now - this.lastCleanupMs > this.minIntervalMs
        ) {
          this.pagesSinceCleanup = 0;
          this.lastCleanupMs = now;
          this.onRebuild?.();
        }
      });
      this.terminal.loadAddon(addon);
      return addon;
    } catch {
      return undefined;
    }
  }
}
