import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const defaultTarget = {
  adapter: { command: process.execPath, args: [join(repo, 'scripts/story-adapter-reference/adapter.mjs')] },
  fixture: { command: process.execPath, args: [join(repo, 'scripts/story-adapter-reference/fixture.mjs')] },
};
// One parameter selects both the adapter and its real-store fixture. No shell parsing.
const target = process.env.TUIC_STORY_ADAPTER_TEST_TARGET
  ? JSON.parse(process.env.TUIC_STORY_ADAPTER_TEST_TARGET) : defaultTarget;
for (const key of ['adapter', 'fixture']) {
  assert.equal(typeof target[key]?.command, 'string', `${key}.command is required`);
  assert.ok(Array.isArray(target[key]?.args), `${key}.args is required`);
}

export function fixture(action, root, fields = {}) {
  const { command, args } = target.fixture;
  const run = spawnSync(command, [...args, action, root, JSON.stringify(fields)], {
    cwd: repo, encoding: 'utf8', windowsHide: true, timeout: 30000,
  });
  assert.equal(run.status, 0, `fixture ${action}: ${run.error || run.stderr}`);
  return JSON.parse(run.stdout);
}

export function connect(t, root) {
  const { command, args } = target.adapter;
  const child = spawn(command, args, { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const exit = once(child, 'exit');
  const pending = new Map();
  const stray = [];
  let diagnostics = '';
  let serial = 0;
  child.stderr.on('data', chunk => { diagnostics += chunk; });
  child.on('error', error => {
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  });
  child.on('exit', (code, signal) => {
    for (const waiter of pending.values()) waiter.reject(new Error(`adapter exited ${code ?? signal}: ${diagnostics}`));
    pending.clear();
  });
  createInterface({ input: child.stdout }).on('line', line => {
    let message;
    try { message = JSON.parse(line); } catch { stray.push(line); return; }
    if (message.method === 'changed' && message.id === undefined) return;
    const waiter = pending.get(message.id);
    if (!waiter) { stray.push(line); return; }
    pending.delete(message.id);
    waiter.resolve(message);
  });
  const rpc = async (method, params = {}) => {
    const id = ++serial;
    const reply = new Promise((resolveReply, reject) => pending.set(id, { resolve: resolveReply, reject }));
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    const message = await reply;
    assert.equal(message.jsonrpc, '2.0');
    assert.equal(message.id, id);
    assert.notEqual(Object.hasOwn(message, 'result'), Object.hasOwn(message, 'error'));
    return message;
  };
  const call = async (method, params) => {
    const reply = await rpc(method, params);
    assert.ok(!reply.error, `${method}: ${JSON.stringify(reply.error)}`);
    return reply.result;
  };
  const shutdown = async () => {
    assert.deepEqual(await call('shutdown'), {});
    const [code, signal] = await exit;
    assert.equal(signal, null);
    assert.equal(code, 0);
    assert.deepEqual(stray, [], 'stdout must contain only protocol messages');
  };
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill();
      await exit;
    }
    assert.deepEqual(stray, [], 'stdout must contain only protocol messages');
  });
  return { rpc, call, shutdown };
}

export async function setup(t) {
  const temp = process.env.TUIC_TEST_TMP_ROOT || join(repo, '.tmp', 'story-adapter-conformance');
  mkdirSync(temp, { recursive: true });
  const root = mkdtempSync(join(temp, 'case-'));
  let seed;
  try { seed = fixture('setup', root); } catch (error) {
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
  const client = connect(t, root);
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const info = await client.call('initialize', { protocol: 1, project_root: root });
  const supports = name => info.capabilities[name] === true;
  const story = () => client.call('get_story', { id: seed.id });
  const apply = async op => {
    const current = await story();
    return client.call('apply', { id: seed.id, expected_version: current.version, op });
  };
  return { ...client, root, seed, info, supports, story, apply };
}
