// Global test setup: signal to transport.ts/invoke.ts that we're in Tauri mode
(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};

// happy-dom ≥20.8 ships a Proxy-based localStorage missing .clear().
// The Proxy traps block both assignment and defineProperty, so we replace
// the entire global with a spec-compliant shim.
if (typeof localStorage !== "undefined" && typeof localStorage.clear !== "function") {
  const store = new Map<string, string>();
  const shim: Storage = {
    get length() { return store.size; },
    clear() { store.clear(); },
    getItem(key: string) { return store.get(key) ?? null; },
    key(index: number) { return [...store.keys()][index] ?? null; },
    removeItem(key: string) { store.delete(key); },
    setItem(key: string, value: string) { store.set(key, String(value)); },
  };
  Object.defineProperty(globalThis, "localStorage", { value: shim, writable: true, configurable: true });
}
