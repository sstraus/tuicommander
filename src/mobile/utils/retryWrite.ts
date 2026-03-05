const BACKOFF_MS = [500, 1000];
const MAX_ATTEMPTS = 3;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Retry a write operation up to MAX_ATTEMPTS times with exponential backoff.
 * First attempt is immediate. Subsequent attempts wait BACKOFF_MS[i-1].
 */
export async function retryWrite(fn: () => Promise<void>): Promise<void> {
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    try {
      await fn();
      return;
    } catch (err) {
      if (attempt === MAX_ATTEMPTS - 1) throw err;
      await delay(BACKOFF_MS[attempt]);
    }
  }
}
