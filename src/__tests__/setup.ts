// Global test setup: signal to transport.ts/invoke.ts that we're in Tauri mode
(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
