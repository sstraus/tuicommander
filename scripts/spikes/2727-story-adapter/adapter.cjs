// Disposable measurement adapter. No production API and no atomic-write claim.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const readline = require('node:readline');
const { spawnSync } = require('node:child_process');
const root = process.env.SPIKE_ROOT;
const wiz = process.env.SPIKE_WIZ;
const { parseFrontmatter } = require(path.join(wiz, 'scripts/lib/cli-utils'));
if (!root.startsWith(path.join(process.env.HOME, 'Gits/.tmp/2727-spike/'))) throw Error('copy required');
const folder = path.join(root, 'stories');
const send = (data) => process.stdout.write(`${JSON.stringify(data)}\n`);
function scan() {
  return fs.readdirSync(folder).filter((f) => f.endsWith('.md')).map((f) => {
    const bytes = fs.readFileSync(path.join(folder, f));
    return { ...parseFrontmatter(bytes.toString()), version: crypto.createHash('sha256').update(bytes).digest('hex'), file: f };
  });
}
let prior = new Map();
let cursor = 0;
let release;
const watcher = fs.watch(folder, (_event, file) => send({ jsonrpc: '2.0', method: 'changed', params: { file } }));
function cli(args) {
  const r = spawnSync(process.execPath, [path.join(wiz, 'scripts/stories-cli.js'), ...args], { cwd: root, encoding: 'utf8', env: { ...process.env, SESSION_ID: '2727-copy-only' } });
  if (r.status !== 0) throw { code: -32002, message: 'refused', data: { status: r.status, reason: r.stdout + r.stderr } };
}
async function dispatch(method, p) {
  if (method === 'initialize') return { protocol: 1, adapter: { id: '2727-spike', version: '0' }, capabilities: { list_stories: true, get_story: true, apply: true, changes: true }, watch_paths: [folder] };
  if (method === 'list_stories') return scan();
  if (method === 'get_story') return scan().find((s) => s.id === p.id);
  if (method === 'changes') {
    const next = new Map(scan().map((s) => [s.id, s.version]));
    const changed = [...new Set([...prior.keys(), ...next.keys()])].filter((id) => prior.get(id) !== next.get(id));
    const rescan_required = p.cursor !== undefined && p.cursor !== cursor;
    prior = next;
    return { changed, cursor: ++cursor, rescan_required };
  }
  if (method === 'spike_release') { release(); return true; }
  if (method === 'apply') {
    const before = scan().find((s) => s.id === p.id);
    if (before.version !== p.expected_version) throw { code: -32001, message: 'conflict' };
    // Controlled barrier proves the check/write gap without timing luck.
    if (p.spike_barrier) await new Promise((resolve) => { release = resolve; send({ jsonrpc: '2.0', method: 'spike_checked' }); });
    if (p.op.kind === 'set_status') cli(['status', p.id, p.op.value]);
    else if (p.op.kind === 'set_title') cli(['update', p.id, 'title', p.op.value]);
    else throw { code: -32601, message: 'unsupported' };
    return scan().find((s) => s.id === p.id);
  }
  if (method === 'shutdown') { watcher.close(); setImmediate(() => process.exit(0)); return true; }
  throw { code: -32601, message: 'unsupported' };
}
readline.createInterface({ input: process.stdin }).on('line', async (line) => {
  let q;
  try { q = JSON.parse(line); send({ jsonrpc: '2.0', id: q.id, result: await dispatch(q.method, q.params || {}) }); }
  catch (e) { send({ jsonrpc: '2.0', id: q?.id, error: { code: e.code || -32603, message: e.message || String(e), data: e.data } }); }
});
