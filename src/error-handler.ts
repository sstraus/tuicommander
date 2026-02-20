import type { AgentType } from "./agents";

/** Error handling strategy */
export type ErrorStrategy = "retry" | "skip" | "abort";

/** Error handling configuration */
export interface ErrorHandlerConfig {
  strategy: ErrorStrategy;
  maxRetries: number;
  baseDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}

/** Default configuration */
export const DEFAULT_ERROR_CONFIG: ErrorHandlerConfig = {
  strategy: "retry",
  maxRetries: 3,
  baseDelayMs: 1000,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
};

/** Error event data */
export interface ErrorEvent {
  sessionId: string;
  agentType: AgentType;
  error: string;
  errorType: ErrorType;
  timestamp: number;
  retryCount: number;
}

/** Error type classification */
export type ErrorType = "rate_limit" | "network" | "auth" | "validation" | "unknown";

/**
 * Classify error type from message.
 *
 * Source of truth: src-tauri/src/error_classification.rs
 * This sync mirror exists because ErrorHandler.handle() calls it synchronously.
 * Keep patterns in sync with the Rust classify_error() function.
 * The Rust command `classify_error_message` is also available via Tauri invoke.
 */
export function classifyError(errorMessage: string): ErrorType {
  const lower = errorMessage.toLowerCase();

  if (lower.includes("rate limit") || lower.includes("too many requests") || lower.includes("quota exceeded") || lower.includes("429")) {
    return "rate_limit";
  }

  if (lower.includes("network error") || lower.includes("connection refused") || lower.includes("timeout") || lower.includes("econnrefused") || lower.includes("etimedout")) {
    return "network";
  }

  if (lower.includes("unauthorized") || lower.includes("authentication failed") || lower.includes("invalid api key")) {
    return "auth";
  }

  if (lower.includes("invalid request") || lower.includes("validation error")) {
    return "validation";
  }

  return "unknown";
}

/**
 * Calculate delay with exponential backoff.
 *
 * Source of truth: src-tauri/src/error_classification.rs
 * This sync mirror exists because ErrorHandler.handle() calls it synchronously.
 * Keep the formula in sync with the Rust calculate_backoff_delay() function.
 * The Rust command `calculate_backoff_delay_cmd` is also available via Tauri invoke.
 */
export function calculateBackoffDelay(
  retryCount: number,
  config: ErrorHandlerConfig
): number {
  const delay = config.baseDelayMs * Math.pow(config.backoffMultiplier, retryCount);
  // Add jitter (10% random variation)
  const jitter = delay * 0.1 * (Math.random() - 0.5);
  return Math.min(delay + jitter, config.maxDelayMs);
}

/** Decision result from error handler */
export interface ErrorDecision {
  action: "retry" | "skip" | "abort";
  delayMs?: number;
  reason: string;
}

/** Error handler class */
export class ErrorHandler {
  private config: ErrorHandlerConfig;
  private retryCounters: Map<string, number> = new Map();

  constructor(config: Partial<ErrorHandlerConfig> = {}) {
    this.config = { ...DEFAULT_ERROR_CONFIG, ...config };
  }

  /** Handle an error and return the decision */
  handle(sessionId: string, errorMessage: string): ErrorDecision {
    const errorType = classifyError(errorMessage);
    const retryCount = this.retryCounters.get(sessionId) || 0;

    switch (this.config.strategy) {
      case "abort":
        return {
          action: "abort",
          reason: "Strategy is set to abort on error",
        };

      case "skip":
        return {
          action: "skip",
          reason: "Strategy is set to skip on error",
        };

      case "retry":
        return this.handleRetry(sessionId, errorType, retryCount);

      default:
        return {
          action: "abort",
          reason: "Unknown strategy",
        };
    }
  }

  /** Handle retry strategy */
  private handleRetry(
    sessionId: string,
    errorType: ErrorType,
    retryCount: number
  ): ErrorDecision {
    // Check if we've exceeded max retries
    if (retryCount >= this.config.maxRetries) {
      this.retryCounters.delete(sessionId);
      return {
        action: "skip",
        reason: `Max retries (${this.config.maxRetries}) exceeded`,
      };
    }

    // Auth errors should not be retried
    if (errorType === "auth") {
      return {
        action: "abort",
        reason: "Authentication errors cannot be retried",
      };
    }

    // Validation errors should not be retried
    if (errorType === "validation") {
      return {
        action: "skip",
        reason: "Validation errors should not be retried",
      };
    }

    // Increment retry counter
    this.retryCounters.set(sessionId, retryCount + 1);

    // Calculate delay
    const delayMs = calculateBackoffDelay(retryCount, this.config);

    return {
      action: "retry",
      delayMs,
      reason: `Retry ${retryCount + 1}/${this.config.maxRetries} after ${Math.round(delayMs)}ms`,
    };
  }

  /** Reset retry counter for a session */
  resetRetryCount(sessionId: string): void {
    this.retryCounters.delete(sessionId);
  }

  /** Get current retry count for a session */
  getRetryCount(sessionId: string): number {
    return this.retryCounters.get(sessionId) || 0;
  }

  /** Update configuration */
  updateConfig(config: Partial<ErrorHandlerConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /** Get current configuration */
  getConfig(): ErrorHandlerConfig {
    return { ...this.config };
  }

  /** Clear all retry counters */
  clearAll(): void {
    this.retryCounters.clear();
  }
}
