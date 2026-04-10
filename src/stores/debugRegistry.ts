/**
 * Debug snapshot registry — stores self-register a snapshot function
 * so that MCP invoke_js can inspect runtime state via window.__TUIC__.store(name).
 *
 * Each store decides what to expose (privacy by design).
 */

const registry = new Map<string, () => unknown>();

/** Register a named snapshot function. Called at store init time. */
export function registerDebugSnapshot(name: string, fn: () => unknown): void {
  registry.set(name, fn);
}

/** Get a snapshot by name, or null if not registered. */
export function getDebugSnapshot(name: string): unknown {
  const fn = registry.get(name);
  return fn ? fn() : null;
}

/** List all registered snapshot names. */
export function listDebugSnapshots(): string[] {
  return [...registry.keys()].sort();
}
