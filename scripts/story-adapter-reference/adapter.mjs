#!/usr/bin/env node
// Reference story-backend adapter for TUIC (protocol 1, docs/backend/story-backend-protocol.md).
// Stores plans and stories in one JSON file. Usage: adapter.mjs [--store <path>]
// Default store: <cwd>/.tuic-reference-stories.json. Each request rereads the file,
// so a second writer's change is visible and an expected_version check sees it.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { withStoreLock } from './store-lock.mjs';

const PROTOCOL = 1;
const STATUSES = new Set(['pending', 'ready', 'in_progress', 'blocked', 'complete']);
const OPS = ['set_status', 'check_criterion', 'add_criterion', 'add_dependency', 'remove_dependency', 'set_fields'];
const storeArg = process.argv.indexOf('--store');
const storePath = resolve(storeArg > 0 ? process.argv[storeArg + 1] : '.tuic-reference-stories.json');

class RpcError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}
const conflict = () => new RpcError(-32001, 'conflict');
const notFound = (id) => new RpcError(-32004, `not_found: ${id}`);
const invalid = (message) => new RpcError(-32602, message);

function load() {
  if (!existsSync(storePath)) return { seq: 0, plans: [{ id: 'default', title: 'Default' }], stories: [] };
  return JSON.parse(readFileSync(storePath, 'utf8'));
}

function save(db) {
  const tmp = `${storePath}.${process.pid}.tmp`;
  writeFileSync(tmp, JSON.stringify(db, null, 2));
  renameSync(tmp, storePath);
}

// The version covers the content only, never the change sequence.
function versioned(record) {
  const { changed_seq: _seq, ...content } = record;
  const version = createHash('sha256').update(JSON.stringify(content)).digest('hex');
  return { ...content, version };
}

function findStory(db, id) {
  const story = db.stories.find((s) => s.id === id);
  if (!story) throw notFound(id);
  return story;
}

function touch(db, story) {
  db.seq += 1;
  story.changed_seq = db.seq;
  save(db);
  return versioned(story);
}

function applyOp(db, story, op) {
  switch (op?.kind) {
    case 'set_status':
      if (!STATUSES.has(op.status)) throw invalid(`unknown status ${op.status}`);
      if (op.status === 'complete' && story.criteria.some((c) => !c.checked)) {
        throw new RpcError(-32002, 'unchecked criteria');
      }
      story.status = op.status;
      break;
    case 'check_criterion':
      if (!story.criteria[op.index]) throw invalid(`no criterion ${op.index}`);
      story.criteria[op.index].checked = true;
      break;
    case 'add_criterion':
      story.criteria.push({ text: String(op.text), checked: false });
      break;
    case 'add_dependency':
      findStory(db, op.id);
      if (!story.dependencies.includes(op.id)) story.dependencies.push(op.id);
      break;
    case 'remove_dependency':
      story.dependencies = story.dependencies.filter((d) => d !== op.id);
      break;
    case 'set_fields':
      for (const key of ['title', 'problem', 'priority']) if (key in op) story[key] = op[key];
      break;
    default:
      throw new RpcError(-32003, `unsupported op ${op?.kind}`);
  }
}

const methods = {
  initialize({ protocol }) {
    if (protocol !== PROTOCOL) throw invalid(`protocol ${protocol} not supported`);
    const capabilities = Object.fromEntries(
      ['list_plans', 'list_stories', 'get_story', 'create_story', 'apply', 'append_note', 'changes', ...OPS]
        .map((name) => [name, true]),
    );
    capabilities.set_fields_problem = true;
    capabilities.set_fields_multiple = true;
    return { protocol: PROTOCOL, adapter: { id: 'tuic-reference', version: '1' }, capabilities, watch_paths: [storePath] };
  },
  list_plans() {
    return { plans: load().plans.map(versioned) };
  },
  list_stories({ plan_id } = {}) {
    const stories = load().stories.filter((s) => plan_id == null || s.plan_id === plan_id);
    return { stories: stories.map(versioned) };
  },
  get_story({ id }) {
    return versioned(findStory(load(), id));
  },
  create_story({ fields }) {
    const db = load();
    if (!fields?.title) throw invalid('title is required');
    const plan_id = fields.plan_id ?? 'default';
    if (!db.plans.some((p) => p.id === plan_id)) throw notFound(plan_id);
    const story = {
      id: `s${db.stories.length + 1}`, plan_id, title: fields.title, status: 'pending',
      priority: fields.priority ?? 2, problem: fields.problem ?? '',
      criteria: (fields.criteria ?? []).map((text) => ({ text, checked: false })),
      dependencies: [], notes: [],
    };
    db.stories.push(story);
    return touch(db, story);
  },
  apply({ id, expected_version, op }) {
    const db = load();
    const story = findStory(db, id);
    if (versioned(story).version !== expected_version) throw conflict();
    applyOp(db, story, op);
    return touch(db, story);
  },
  append_note({ id, text }) {
    const db = load();
    const story = findStory(db, id);
    story.notes.push({ at: new Date().toISOString(), text: String(text) });
    return touch(db, story);
  },
  changes({ cursor }) {
    const db = load();
    const after = Number.parseInt(cursor ?? '', 10);
    if (!Number.isInteger(after) || after > db.seq) return { rescan_required: true, cursor: String(db.seq) };
    const changed = db.stories.filter((s) => (s.changed_seq ?? 0) > after).map((s) => s.id);
    return { changed, cursor: String(db.seq) };
  },
};

const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);

createInterface({ input: process.stdin }).on('line', (line) => {
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    send({ id: null, error: { code: -32700, message: 'parse error' } });
    return;
  }
  const { id, method, params } = request;
  if (method === 'shutdown') {
    // Pipe writes are asynchronous: exit only after the answer is flushed.
    process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, result: {} })}\n`, () => process.exit(0));
    return;
  }
  const handler = methods[method];
  if (!handler) {
    send({ id, error: { code: -32601, message: `method not found: ${method}` } });
    return;
  }
  try {
    const result = ['create_story', 'apply', 'append_note'].includes(method)
      ? withStoreLock(storePath, () => handler(params ?? {}))
      : handler(params ?? {});
    send({ id, result });
  } catch (error) {
    const code = error instanceof RpcError ? error.code : -32603;
    send({ id, error: { code, message: error.message } });
  }
});
