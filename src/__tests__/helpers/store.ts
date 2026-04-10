import { createRoot } from "solid-js";

import type { AgentType } from "../../agents";

/** Default terminal data for tests — override any field via spread. */
export function makeTerminal(overrides: Partial<{
  sessionId: string | null;
  fontSize: number;
  name: string;
  cwd: string | null;
  awaitingInput: "question" | "error" | null;
  agentType: AgentType | null;
  agentSessionId: string | null;
}> = {}) {
  return {
    sessionId: null as string | null,
    fontSize: 14,
    name: "Test",
    cwd: null as string | null,
    awaitingInput: null as "question" | "error" | null,
    ...overrides,
  };
}

/**
 * Run a test function inside a SolidJS reactive scope.
 * Disposes the scope after the function completes (even on throw).
 */
export function testInScope<T>(fn: () => T): T {
  let dispose!: () => void;
  try {
    return createRoot((d) => {
      dispose = d;
      return fn();
    });
  } finally {
    dispose();
  }
}

/**
 * Run an async test function inside a SolidJS reactive scope.
 * Disposes the scope after the promise settles (even on rejection).
 */
export async function testInScopeAsync<T>(fn: () => Promise<T>): Promise<T> {
  let dispose!: () => void;
  try {
    return await createRoot((d) => {
      dispose = d;
      return fn();
    });
  } finally {
    dispose();
  }
}
