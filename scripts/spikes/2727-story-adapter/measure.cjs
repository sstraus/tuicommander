// Reproducible single-process measurement; only the disposable copy is writable.
const fs = require('node:fs');
const path = require('node:path');
const { spawn, spawnSync, execFileSync } = require('node:child_process');
const readline = require('node:readline');
const { performance } = require('node:perf_hooks');
const assert = require('node:assert/strict');
const source = process.argv[2];
const base = path.join(process.env.HOME, 'Gits/.tmp/2727-spike');
const root = path.join(base, `run-${Date.now()}`);
fs.mkdirSync(path.join(root, 'stories'), { recursive: true });
const storyDir = path.join(source, 'stories');
const active = fs.readdirSync(storyDir).filter((f) => f.endsWith('.md')).sort();
const archived = fs.readdirSync(path.join(storyDir, 'archive')).filter((f) => f.endsWith('.md')).sort().map((f) => `archive/${f}`);
const files = [...active, ...archived].slice(0, 385);
assert.equal(files.length, 385, '385 real source files required');
for (const f of files) fs.copyFileSync(path.join(source, 'stories', f), path.join(root, 'stories', path.basename(f)));
execFileSync('git', ['init', '-q', root]);
const wiz = execFileSync('wiz-run', ['--root'], { encoding: 'utf8' }).trim();
const child = spawn(process.execPath, [path.join(__dirname, 'adapter.cjs')], { env: { ...process.env, SPIKE_ROOT: root, SPIKE_WIZ: wiz }, stdio: ['pipe', 'pipe', 'inherit'] });
let seq = 0;
const pending = new Map();
const notifications = new Map();
readline.createInterface({ input: child.stdout }).on('line', (line) => {
  const q = JSON.parse(line);
  if (q.id) { pending.get(q.id)(q); pending.delete(q.id); }
  else { notifications.get(q.method)?.(q); notifications.delete(q.method); }
});
function rpc(method, params = {}) {
  return new Promise((resolve) => { const id = ++seq; pending.set(id, resolve); child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`); });
}
function notice(method) { return new Promise((resolve) => notifications.set(method, resolve)); }
function cli(args) {
  const r = spawnSync(process.execPath, [path.join(wiz, 'scripts/stories-cli.js'), ...args], { cwd: root, encoding: 'utf8', env: { ...process.env, SESSION_ID: '2727-copy-only' } });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  return r.stdout;
}
function stats(values) {
  values.sort((a, b) => a - b);
  return { n: values.length, median_ms: values[Math.floor(values.length / 2)], p95_ms: values[Math.ceil(values.length * .95) - 1], max_ms: values.at(-1) };
}
(async () => {
  try {
    const out = { root, node: process.version, platform: `${process.platform}/${process.arch}`, wiz, source_head: execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), count: files.length };
    let start = performance.now();
    out.initialize = await rpc('initialize');
    out.cold_initialize_ms = performance.now() - start;
    const listed = (await rpc('list_stories')).result;
    assert.equal(listed.length, 385);
    const chosen = listed.find((s) => s.status === 'pending' || s.status === 'ready');
    assert.ok(chosen);
    out.story_id = chosen.id;
    for (const method of ['initialize', 'get_story', 'list_stories', 'changes']) {
      const times = [];
      let cursor;
      for (let i = 0; i < 50; i++) {
        start = performance.now();
        const r = await rpc(method, { id: chosen.id, cursor });
        assert.ok(!r.error);
        cursor = r.result.cursor;
        times.push(performance.now() - start);
      }
      out[method] = stats(times);
    }
    const watches = [];
    for (let i = 0; i < 10; i++) {
      const changed = notice('changed');
      start = performance.now();
      fs.appendFileSync(path.join(root, 'stories', chosen.file), `\n<!-- watch ${i} -->\n`);
      await changed;
      watches.push(performance.now() - start);
    }
    out.watch_hint = stats(watches);
    let current = (await rpc('get_story', { id: chosen.id })).result;
    cli(['update', chosen.id, 'title', 'Parallel external title']);
    const beforeStale = (await rpc('get_story', { id: chosen.id })).result;
    out.stale_version = await rpc('apply', { id: chosen.id, expected_version: current.version, op: { kind: 'set_title', value: 'Stale title' } });
    assert.equal(out.stale_version.error.message, 'conflict');
    assert.equal((await rpc('get_story', { id: chosen.id })).result.version, beforeStale.version);
    out.race = [];
    for (let i = 0; i < 5; i++) {
      current = (await rpc('get_story', { id: chosen.id })).result;
      const checked = notice('spike_checked');
      const applying = rpc('apply', { id: chosen.id, expected_version: current.version, spike_barrier: true, op: { kind: 'set_title', value: `Adapter title ${i}` } });
      await checked;
      start = performance.now();
      cli(['update', chosen.id, 'title', `External winner ${i}`]);
      const external = (await rpc('get_story', { id: chosen.id })).result;
      await rpc('spike_release');
      const result = await applying;
      out.race.push({ external_title: external.title, final_title: result.result?.title, error: result.error || null, elapsed_ms: performance.now() - start });
      assert.equal(external.title, `External winner ${i}`);
      assert.equal(result.result.title, `Adapter title ${i}`);
    }
    current = (await rpc('get_story', { id: chosen.id })).result;
    start = performance.now();
    out.complete = await rpc('apply', { id: chosen.id, expected_version: current.version, op: { kind: 'set_status', value: 'complete' } });
    out.complete_ms = performance.now() - start;
    assert.equal(out.complete.error?.message, 'refused');
    out.final_status = (await rpc('get_story', { id: chosen.id })).result.status;
    out.verdict = 'NO-GO: check then CLI write is not atomic; competing title overwritten in 5/5 controlled races';
    fs.writeFileSync(path.join(root, 'measurements.json'), `${JSON.stringify(out, null, 2)}\n`);
    process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
  } finally {
    await rpc('shutdown');
    child.stdin.end();
  }
})().catch((e) => { process.stderr.write(`${e.stack}\n`); process.exitCode = 1; });
