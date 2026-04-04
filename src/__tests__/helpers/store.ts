import { createRoot } from "solid-js";

/** Default terminal data for tests — override any field via spread. */
export function makeTerminal(overrides: Partial<{
  sessionId: string | null;
  fontSize: number;
  name: string;
  cwd: string | null;
  awaitingInput: "question" | "error" | null;
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
 * Disposes the scope after the function completes.
 */
export function testInScope<T>(fn: () => T): T {
  let result!: T;
  let dispose!: () => void;
  createRoot((d) => {
    dispose = d;
    result = fn();
  });
  dispose();
  return result;
}

/**
 * Run an async test function inside a SolidJS reactive scope.
 * Disposes the scope after the function completes.
 */
export async function testInScopeAsync<T>(fn: () => Promise<T>): Promise<T> {
  let result!: Promise<T>;
  let dispose!: () => void;
  createRoot((d) => {
    dispose = d;
    result = fn();
  });
  try {
    return await result;
  } finally {
    dispose();
  }
}
