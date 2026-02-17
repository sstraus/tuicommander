import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";
import {
  ErrorHandler,
  type ErrorHandlerConfig,
  type ErrorStrategy,
  type ErrorDecision,
  DEFAULT_ERROR_CONFIG,
} from "../error-handler";

/** Error handling state */
interface ErrorHandlingState {
  config: ErrorHandlerConfig;
  activeRetries: Record<string, RetryInfo>;
}

/** Active retry information */
export interface RetryInfo {
  sessionId: string;
  retryCount: number;
  nextRetryAt: number | null;
  lastError: string;
}

const LEGACY_STORAGE_KEY = "tui-commander-error-handling";

/** Persist config to Rust backend (fire-and-forget) */
function saveConfig(config: ErrorHandlerConfig): void {
  invoke("save_ui_prefs", {
    config: { error_handling: { strategy: config.strategy, max_retries: config.maxRetries } },
  }).catch((err) => console.debug("Failed to save error config:", err));
}

/** Create error handling store */
function createErrorHandlingStore() {
  const handler = new ErrorHandler({ ...DEFAULT_ERROR_CONFIG });

  const [state, setState] = createStore<ErrorHandlingState>({
    config: { ...DEFAULT_ERROR_CONFIG },
    activeRetries: {},
  });

  const actions = {
    /** Load config from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = JSON.parse(legacy);
            await invoke("save_ui_prefs", {
              config: { error_handling: { strategy: parsed.strategy, max_retries: parsed.maxRetries } },
            });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const prefs = await invoke<{ error_handling?: { strategy?: string; max_retries?: number } }>("load_ui_prefs");
        if (prefs?.error_handling) {
          const eh = prefs.error_handling;
          const config: ErrorHandlerConfig = {
            ...DEFAULT_ERROR_CONFIG,
            strategy: (eh.strategy as ErrorStrategy) ?? DEFAULT_ERROR_CONFIG.strategy,
            maxRetries: eh.max_retries ?? DEFAULT_ERROR_CONFIG.maxRetries,
          };
          setState("config", config);
          handler.updateConfig(config);
        }
      } catch (err) {
        console.debug("Failed to hydrate error config:", err);
      }
    },

    /** Update error handling configuration */
    updateConfig(config: Partial<ErrorHandlerConfig>): void {
      const newConfig = { ...state.config, ...config };
      setState("config", newConfig);
      handler.updateConfig(newConfig);
      saveConfig(newConfig);
    },

    /** Set error handling strategy */
    setStrategy(strategy: ErrorStrategy): void {
      actions.updateConfig({ strategy });
    },

    /** Set max retries */
    setMaxRetries(maxRetries: number): void {
      actions.updateConfig({ maxRetries: Math.max(0, Math.min(10, maxRetries)) });
    },

    /** Set base delay */
    setBaseDelay(baseDelayMs: number): void {
      actions.updateConfig({ baseDelayMs: Math.max(100, Math.min(60000, baseDelayMs)) });
    },

    /** Handle an error for a session */
    handleError(sessionId: string, errorMessage: string): ErrorDecision {
      const decision = handler.handle(sessionId, errorMessage);

      if (decision.action === "retry" && decision.delayMs) {
        setState("activeRetries", sessionId, {
          sessionId,
          retryCount: handler.getRetryCount(sessionId),
          nextRetryAt: Date.now() + decision.delayMs,
          lastError: errorMessage,
        });
      } else {
        // Clear retry info on skip or abort
        const { [sessionId]: _, ...rest } = state.activeRetries;
        setState("activeRetries", reconcile(rest));
      }

      return decision;
    },

    /** Clear retry info for a session */
    clearRetry(sessionId: string): void {
      handler.resetRetryCount(sessionId);
      const { [sessionId]: _, ...rest } = state.activeRetries;
      setState("activeRetries", reconcile(rest));
    },

    /** Get retry info for a session */
    getRetryInfo(sessionId: string): RetryInfo | undefined {
      return state.activeRetries[sessionId];
    },

    /** Get all active retries */
    getActiveRetries(): RetryInfo[] {
      return Object.values(state.activeRetries);
    },

    /** Check if session is in retry mode */
    isRetrying(sessionId: string): boolean {
      return sessionId in state.activeRetries;
    },

    /** Reset all */
    resetAll(): void {
      handler.clearAll();
      setState("activeRetries", reconcile({}));
    },

    /** Reset configuration to defaults */
    resetConfig(): void {
      const defaults = { ...DEFAULT_ERROR_CONFIG };
      setState("config", defaults);
      handler.updateConfig(defaults);
      saveConfig(defaults);
    },

    /** Get handler instance for advanced usage */
    getHandler(): ErrorHandler {
      return handler;
    },
  };

  return { state, ...actions };
}

export const errorHandlingStore = createErrorHandlingStore();
