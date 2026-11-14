// The version check and store write must be one inter-process critical section.
import { closeSync, openSync, unlinkSync } from 'node:fs';

const pause = new Int32Array(new SharedArrayBuffer(4));
export function withStoreLock(storePath, write) {
  const lockPath = `${storePath}.lock`;
  const deadline = Date.now() + 5000;
  let fd;
  for (;;) {
    try {
      fd = openSync(lockPath, 'wx');
      break;
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      if (Date.now() >= deadline) throw new Error(`store is locked: ${lockPath}; remove a crashed writer's lock only after stopping all writers`);
      Atomics.wait(pause, 0, 0, 10);
    }
  }
  try {
    return write();
  } finally {
    closeSync(fd);
    unlinkSync(lockPath);
  }
  // DEFERRED (2026-11-12) — automatic crash-lock recovery needs a native advisory
  // lock; never expire a live writer's lock by age and permit a lost update.
}
