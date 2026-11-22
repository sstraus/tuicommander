#!/usr/bin/env node
// Compare two discovery E2E reports (A = baseline, B = candidate) per agent.
// B fails when it loses capability (a task A solved that B no longer solves as
// often), needs more discovery hops or tool calls to get there, or makes more
// failed calls. Token figures are reported, not gated.
import { readFileSync } from "node:fs";

const [aPath, bPath] = process.argv.slice(2);
if (!aPath || !bPath) { console.error("usage: compare.mjs <A.json> <B.json>"); process.exit(2); }
const A = JSON.parse(readFileSync(aPath, "utf8"));
const B = JSON.parse(readFileSync(bPath, "utf8"));
// Allowed slack per run for model noise: half a hop, half a failed call.
const SLACK = Number(process.env.DISCOVERY_E2E_SLACK ?? 0.5);

const mean = (xs) => (xs.length ? xs.reduce((s, x) => s + (x ?? 0), 0) / xs.length : 0);
function stats(report, agent, task) {
  const rows = report.runs.filter((r) => r.agent === agent && (task === undefined || r.task === task));
  return {
    n: rows.length,
    success: mean(rows.map((r) => (r.ok ? 1 : 0))),
    discovery: mean(rows.map((r) => r.discovery)),
    calls: mean(rows.map((r) => r.tool_calls)),
    errors: mean(rows.map((r) => r.errors)),
    tokens: mean(rows.map((r) => r.tokens)),
    cost: mean(rows.map((r) => r.cost)),
  };
}

const failures = [];
const agents = [...new Set(B.runs.map((r) => r.agent))];
const fmt = (x, d = 2) => Number(x).toFixed(d);
for (const agent of agents) {
  const tasks = [...new Set(B.runs.filter((r) => r.agent === agent).map((r) => r.task))];
  console.log(`\n## ${agent}\n`);
  console.log("| task | success A→B | discovery A→B | calls A→B | errors A→B | tokens A→B |");
  console.log("|---|---|---|---|---|---|");
  for (const task of [...tasks, undefined]) {
    const a = stats(A, agent, task); const b = stats(B, agent, task);
    const label = task ?? "**all**";
    console.log(`| ${label} | ${fmt(a.success)}→${fmt(b.success)} | ${fmt(a.discovery)}→${fmt(b.discovery)} | ${fmt(a.calls)}→${fmt(b.calls)} | ${fmt(a.errors)}→${fmt(b.errors)} | ${Math.round(a.tokens)}→${Math.round(b.tokens)} |`);
    if (!a.n || !b.n) { failures.push(`${agent}/${label}: missing runs (A ${a.n}, B ${b.n})`); continue; }
    if (b.success < a.success) failures.push(`${agent}/${label}: capability lost, success ${fmt(a.success)} → ${fmt(b.success)}`);
    if (task !== undefined && b.success === 0) failures.push(`${agent}/${label}: never solved in B`);
    if (b.discovery > a.discovery + SLACK) failures.push(`${agent}/${label}: more discovery hops ${fmt(a.discovery)} → ${fmt(b.discovery)}`);
    if (b.calls > a.calls + SLACK) failures.push(`${agent}/${label}: more tool calls ${fmt(a.calls)} → ${fmt(b.calls)}`);
    if (b.errors > a.errors + SLACK) failures.push(`${agent}/${label}: more failed calls ${fmt(a.errors)} → ${fmt(b.errors)}`);
  }
}
console.log(failures.length ? `\nFAIL\n- ${failures.join("\n- ")}` : "\nPASS: no capability, iteration or error regression");
process.exit(failures.length ? 1 : 0);
