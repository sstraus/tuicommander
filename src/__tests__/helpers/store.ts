import { createRoot } from "solid-js";

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
