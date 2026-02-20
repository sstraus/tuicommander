/**
 * Platform detection utilities for cross-platform compatibility
 */

export type Platform = "macos" | "windows" | "linux" | "unknown";

/**
 * Detect the current platform
 * Uses navigator.platform as it's synchronous and works in Tauri webview
 */
export function detectPlatform(): Platform {
  const platform = navigator.platform.toLowerCase();

  if (platform.includes("mac")) {
    return "macos";
  }
  if (platform.includes("win")) {
    return "windows";
  }
  if (platform.includes("linux") || platform.includes("x11")) {
    return "linux";
  }

  return "unknown";
}

/**
 * Check if running on macOS
 */
export function isMacOS(): boolean {
  return detectPlatform() === "macos";
}

/**
 * Check if running on Windows
 */
export function isWindows(): boolean {
  return detectPlatform() === "windows";
}

/**
 * Check if running on Linux
 */
export function isLinux(): boolean {
  return detectPlatform() === "linux";
}

/**
 * Get the modifier key symbol for the current platform
 * macOS: ⌘ (Command), Windows/Linux: Ctrl
 */
export function getModifierSymbol(): string {
  return isMacOS() ? "\u2318" : "Ctrl+";
}

/**
 * Check if the quick switcher activation keys are pressed
 * macOS: Cmd+Ctrl, Windows/Linux: Ctrl+Alt
 */
export function isQuickSwitcherActive(e: KeyboardEvent): boolean {
  if (isMacOS()) {
    return e.metaKey && e.ctrlKey;
  }
  return e.ctrlKey && e.altKey;
}

/**
 * Check if the quick switcher release key was pressed
 * macOS: Meta or Control released, Windows/Linux: Control or Alt released
 */
export function isQuickSwitcherRelease(e: KeyboardEvent): boolean {
  if (isMacOS()) {
    return e.key === "Meta" || e.key === "Control";
  }
  return e.key === "Control" || e.key === "Alt";
}

/**
 * Shorten a path for display by replacing the home directory with ~
 * On macOS/Linux: /Users/name/foo → ~/foo, /home/name/foo → ~/foo
 * On Windows: paths are returned unchanged (~ is not a shell convention)
 */
export function shortenHomePath(path: string): string {
  if (isWindows()) return path;
  // Match /Users/<name>/ (macOS) or /home/<name>/ (Linux)
  const match = path.match(/^(\/(?:Users|home)\/[^/]+)(\/.*)?$/);
  if (match) {
    return "~" + (match[2] ?? "");
  }
  return path;
}

/**
 * Apply platform-specific CSS class to document root
 * Call this on app initialization
 */
export function applyPlatformClass(): Platform {
  const platform = detectPlatform();
  document.documentElement.classList.add(`platform-${platform}`);
  return platform;
}
