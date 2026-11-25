#!/usr/bin/env node
// Black-box workflow E2E. Needs a running `make agent-test` instance; see docs/workflows/testing/e2e.md.
import { scenarios } from "./scenarios.mjs";
import { http, SOCKET } from "./lib.mjs";

const only = process.argv.slice(2);
const health = await http("GET", "/health").catch((e) => ({ status: 0, body: String(e) }));
if (health.status !== 200) {
  console.error(`no agent-test instance on ${SOCKET}: ${JSON.stringify(health.body)}\nstart one with: make agent-test`);
  process.exit(2);
}
console.log(`instance ${health.body.build?.sha256?.slice(0, 12)} socket ${SOCKET}`);
let failed = 0;
const findings = [];
for (const [name, fn] of Object.entries(scenarios)) {
  if (only.length && !only.some((o) => name.startsWith(o))) continue;
  const cleanups = []; const notes = [];
  const ctx = { cleanup: (f) => cleanups.push(f), note: (m) => notes.push(m), finding: (m) => findings.push(`${name}: ${m}`) };
  const t0 = Date.now();
  try {
    await fn(ctx);
    console.log(`PASS ${name} (${Date.now() - t0} ms)`);
  } catch (e) {
    failed++;
    console.log(`FAIL ${name}\n  ${e.stack?.split("\n").slice(0, 30).join("\n  ")}`);
  }
  for (const n of notes) console.log(`  note: ${n}`);
  for (const c of cleanups.reverse()) await Promise.resolve(c()).catch((e) => console.log(`  cleanup error: ${e}`));
}
for (const f of findings) console.log(`FINDING ${f}`);
process.exit(failed ? 1 : 0);
