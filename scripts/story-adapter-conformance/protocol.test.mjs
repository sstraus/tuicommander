// Real-process protocol checks. Every test names the observable contract it protects.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setup, connect, fixture } from './harness.mjs';

const options = { timeout: 60000 };
const check = (name, fn) => test(name, options, fn);
const error = (reply, codes) => {
  assert.ok(reply.error, `expected error, got ${JSON.stringify(reply)}`);
  assert.ok(codes.includes(reply.error.code), JSON.stringify(reply.error));
  assert.equal(typeof reply.error.message, 'string');
  assert.ok(reply.error.message.length);
};
async function operation(t, op, verify, capability = op.kind) {
  const c = await setup(t);
  const before = await c.story();
  const params = { id: before.id, expected_version: before.version, op };
  const reply = await c.rpc('apply', params);
  if (!c.supports(capability)) {
    error(reply, [-32003, -32601]);
    assert.deepEqual(await c.story(), before, 'unsupported operation must not mutate');
    t.diagnostic(`${capability}: unsupported refusal verified`);
    return;
  }
  assert.ok(!reply.error, JSON.stringify(reply.error));
  await verify(reply.result, before, c);
}

check('initialize negotiates protocol 1 and declares method capabilities', async t => {
  const c = await setup(t);
  assert.equal(c.info.protocol, 1);
  for (const key of ['id', 'version']) assert.ok(typeof c.info.adapter[key] === 'string' && c.info.adapter[key]);
  assert.equal(typeof c.info.capabilities, 'object');
  for (const value of Object.values(c.info.capabilities)) assert.equal(typeof value, 'boolean');
  assert.ok(Array.isArray(c.info.watch_paths ?? []));
});

check('a mismatched protocol is refused instead of silently negotiated', async t => {
  const c = await setup(t);
  const other = connect(t, c.root);
  error(await other.rpc('initialize', { protocol: 2, project_root: c.root }), [-32602, -32002]);
});

check('plan records contain opaque versions and include the seeded story plan', async t => {
  const c = await setup(t);
  const { plans } = await c.call('list_plans');
  const plan = plans.find(p => p.id === c.seed.plan_id);
  assert.ok(plan, JSON.stringify(plans));
  assert.ok(typeof plan.version === 'string' && plan.version);
  assert.ok(typeof plan.title === 'string' && plan.title);
});

check('list_stories projects real seeded fields rather than an empty or unrelated store', async t => {
  const c = await setup(t);
  const { stories } = await c.call('list_stories');
  const story = stories.find(s => s.id === c.seed.id);
  assert.ok(story);
  assert.equal(story.title, 'Seed story');
  assert.equal(story.priority, 2);
  assert.equal(story.status, 'pending');
  assert.equal(story.plan_id, c.seed.plan_id);
  assert.equal(story.problem, 'A persisted story for protocol conformance');
  assert.deepEqual(story.criteria.map(({ text, checked }) => ({ text, checked })), [
    { text: 'first criterion', checked: false }, { text: 'second criterion', checked: false },
  ]);
  assert.deepEqual(story.dependencies, []);
  assert.deepEqual(story.notes, []);
});

check('a matching plan filter retains its stories', async t => {
  const c = await setup(t);
  const { stories } = await c.call('list_stories', { plan_id: c.seed.plan_id });
  assert.ok(stories.some(s => s.id === c.seed.id));
  assert.ok(stories.every(s => s.plan_id === c.seed.plan_id));
});

check('a different plan filter does not leak stories from another plan', async t => {
  const c = await setup(t);
  assert.deepEqual(await c.call('list_stories', { plan_id: 'absent-conformance-plan' }), { stories: [] });
});

check('get_story returns the same record as list_stories', async t => {
  const c = await setup(t);
  const { stories } = await c.call('list_stories');
  assert.deepEqual(await c.story(), stories.find(s => s.id === c.seed.id));
});

check('an absent story answers not_found without killing the process', async t => {
  const c = await setup(t);
  error(await c.rpc('get_story', { id: 'missing-story' }), [-32004]);
  assert.equal((await c.story()).id, c.seed.id);
});

check('reads do not change a record version', async t => {
  const c = await setup(t);
  assert.deepEqual(await c.story(), await c.story());
});

check('a new adapter process reads persisted writes, not process-local copies', async t => {
  const c = await setup(t);
  if (!c.supports('set_fields')) { t.skip('set_fields not declared'); return; }
  const updated = await c.apply({ kind: 'set_fields', title: 'Persisted title' });
  await c.shutdown();
  const next = connect(t, c.root);
  await next.call('initialize', { protocol: 1, project_root: c.root });
  assert.deepEqual(await next.call('get_story', { id: c.seed.id }), updated);
});

check('create_story follows its declared capability and preserves supplied content', async t => {
  const c = await setup(t);
  const fields = { plan_id: c.seed.plan_id, title: 'Created through protocol', problem: 'Persisted create operation', priority: 1, criteria: ['created criterion'] };
  const reply = await c.rpc('create_story', { fields });
  if (!c.supports('create_story')) {
    error(reply, [-32003, -32601]);
    t.diagnostic('create_story: unsupported refusal verified; fixture supplies existing stories');
    return;
  }
  assert.ok(!reply.error, JSON.stringify(reply.error));
  const s = reply.result;
  assert.ok(s.id && s.version);
  assert.equal(s.title, fields.title);
  assert.equal(s.status, 'pending');
  assert.equal(s.plan_id, fields.plan_id);
  assert.equal(s.priority, 1);
  assert.deepEqual(s.criteria, [{ text: 'created criterion', checked: false }]);
  assert.deepEqual(await c.call('get_story', { id: s.id }), s);
  assert.ok((await c.call('list_stories')).stories.some(item => item.id === s.id));
});

check('a stale expected_version conflicts and cannot overwrite a successful write', async t => {
  const c = await setup(t);
  if (!c.supports('set_fields')) { t.skip('set_fields not declared'); return; }
  const before = await c.story();
  const updated = await c.apply({ kind: 'set_fields', title: 'First writer' });
  assert.notEqual(updated.version, before.version);
  error(await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'set_fields', title: 'Lost update' } }), [-32001]);
  assert.equal((await c.story()).title, 'First writer');
});

check('an external store write invalidates the earlier expected_version', async t => {
  const c = await setup(t);
  if (!c.supports('set_fields')) { t.skip('set_fields not declared'); return; }
  const before = await c.story();
  fixture('change', c.root, { id: before.id, title: 'External writer' });
  const after = await c.story();
  assert.equal(after.title, 'External writer');
  assert.notEqual(after.version, before.version);
  error(await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'set_fields', title: 'Overwrite external writer' } }), [-32001]);
  assert.equal((await c.story()).title, 'External writer');
});

check('apply to an absent id answers not_found rather than creating it', async t => {
  const c = await setup(t);
  error(await c.rpc('apply', { id: 'missing-story', expected_version: (await c.story()).version, op: { kind: 'set_status', status: 'ready' } }), [-32004]);
});

check('an unknown apply operation answers unsupported and leaves the record intact', async t => {
  const c = await setup(t);
  const before = await c.story();
  error(await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'unknown-conformance-op' } }), [-32003, -32601]);
  assert.deepEqual(await c.story(), before);
});

check('criterion indices are zero-based and do not check a sibling criterion', async t => {
  await operation(t, { kind: 'check_criterion', index: 1 }, async (s, before, c) => {
    assert.equal(s.criteria[0].checked, false);
    assert.equal(s.criteria[1].checked, true);
    assert.notEqual(s.version, before.version);
    assert.equal((await c.apply({ kind: 'check_criterion', index: 0 })).criteria[0].checked, true);
  });
});

check('out-of-range criterion indices fail without changing any criterion', async t => {
  const c = await setup(t);
  if (!c.supports('check_criterion')) { t.skip('check_criterion not declared'); return; }
  const before = await c.story();
  for (const index of [-1, 2]) error(await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'check_criterion', index } }), [-32004, -32602, -32002]);
  assert.deepEqual(await c.story(), before);
});

check('add_criterion appends an unchecked item without replacing existing criteria', async t => {
  await operation(t, { kind: 'add_criterion', text: 'third criterion' }, async (s, before) => {
    assert.deepEqual(s.criteria.slice(0, 2), before.criteria);
    assert.equal(s.criteria[2].text, 'third criterion');
    assert.equal(s.criteria[2].checked, false);
    assert.notEqual(s.version, before.version);
  });
});

check('set_fields title changes title without changing the story identity or criteria', async t => {
  await operation(t, { kind: 'set_fields', title: 'Renamed story' }, async (s, before) => {
    assert.equal(s.id, before.id);
    assert.equal(s.title, 'Renamed story');
    assert.deepEqual(s.criteria, before.criteria);
    assert.notEqual(s.version, before.version);
  });
});

check('set_fields priority preserves the protocol numeric priority', async t => {
  await operation(t, { kind: 'set_fields', priority: 1 }, async s => assert.equal(s.priority, 1));
});

check('problem edits follow the declared field capability rather than assuming a reference store', async t => {
  await operation(t, { kind: 'set_fields', problem: 'Changed problem statement' }, async s => assert.equal(s.problem, 'Changed problem statement'), 'set_fields_problem');
});

check('multi-field writes follow the declared atomic field capability', async t => {
  await operation(t, { kind: 'set_fields', title: 'Atomic fields', priority: 3 }, async s => {
    assert.equal(s.title, 'Atomic fields');
    assert.equal(s.priority, 3);
  }, 'set_fields_multiple');
});

check('add_dependency links the real fixture story without replacing other fields', async t => {
  const c = await setup(t);
  const dep = fixture('create', c.root, { title: 'Dependency' });
  const before = await c.story();
  const reply = await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'add_dependency', id: dep.id } });
  if (!c.supports('add_dependency')) { error(reply, [-32003, -32601]); return; }
  assert.ok(!reply.error, JSON.stringify(reply.error));
  assert.deepEqual(reply.result.dependencies, [dep.id]);
  assert.equal(reply.result.title, before.title);
});

check('adding the same dependency twice does not duplicate the edge', async t => {
  const c = await setup(t);
  if (!c.supports('add_dependency')) { t.skip('add_dependency not declared'); return; }
  const dep = fixture('create', c.root, { title: 'Dependency' });
  await c.apply({ kind: 'add_dependency', id: dep.id });
  assert.deepEqual((await c.apply({ kind: 'add_dependency', id: dep.id })).dependencies, [dep.id]);
});

check('remove_dependency removes only the named edge', async t => {
  const c = await setup(t);
  if (!c.supports('add_dependency') || !c.supports('remove_dependency')) { t.skip('dependency writes not declared'); return; }
  const a = fixture('create', c.root, { title: 'Dependency A' });
  const b = fixture('create', c.root, { title: 'Dependency B' });
  await c.apply({ kind: 'add_dependency', id: a.id });
  await c.apply({ kind: 'add_dependency', id: b.id });
  assert.deepEqual((await c.apply({ kind: 'remove_dependency', id: a.id })).dependencies, [b.id]);
});

check('append_note persists the note and changes the record version', async t => {
  const c = await setup(t);
  const before = await c.story();
  const reply = await c.rpc('append_note', { id: before.id, text: 'Conformance note' });
  if (!c.supports('append_note')) { error(reply, [-32003, -32601]); return; }
  assert.ok(!reply.error, JSON.stringify(reply.error));
  assert.equal(reply.result.notes.at(-1).text, 'Conformance note');
  assert.ok(reply.result.notes.at(-1).at);
  assert.notEqual(reply.result.version, before.version);
  assert.deepEqual(await c.story(), reply.result);
});

check('a null change cursor requests a baseline rescan', async t => {
  const c = await setup(t);
  const result = await c.call('changes', { cursor: null });
  assert.equal(result.rescan_required, true);
  assert.ok(typeof result.cursor === 'string');
});

check('an unusable change cursor requests a rescan instead of missing writes', async t => {
  const c = await setup(t);
  assert.equal((await c.call('changes', { cursor: 'unknown-conformance-generation' })).rescan_required, true);
});

check('changes since a known cursor includes external store edits', async t => {
  const c = await setup(t);
  const baseline = await c.call('changes', { cursor: null });
  fixture('change', c.root, { id: c.seed.id, title: 'Changed outside adapter' });
  const result = await c.call('changes', { cursor: baseline.cursor });
  if (result.rescan_required) { t.diagnostic('adapter requested a conservative rescan'); return; }
  assert.ok(result.changed.includes(c.seed.id), JSON.stringify(result));
  assert.ok(typeof result.cursor === 'string');
});

check('an unchanged store yields no changed ids from a current cursor', async t => {
  const c = await setup(t);
  const baseline = await c.call('changes', { cursor: null });
  const result = await c.call('changes', { cursor: baseline.cursor });
  if (result.rescan_required) { t.diagnostic('adapter requested a conservative rescan'); return; }
  assert.deepEqual(result.changed, []);
});

check('complete refuses unchecked criteria without changing status', async t => {
  const c = await setup(t);
  if (!c.supports('set_status')) { t.skip('set_status not declared'); return; }
  const before = await c.story();
  error(await c.rpc('apply', { id: before.id, expected_version: before.version, op: { kind: 'set_status', status: 'complete' } }), [-32002]);
  assert.equal((await c.story()).status, 'pending');
});

check('complete persists after real backend prerequisites and criterion checks', async t => {
  const c = await setup(t);
  if (!c.supports('set_status') || !c.supports('check_criterion')) { t.skip('completion writes not declared'); return; }
  await c.apply({ kind: 'check_criterion', index: 0 });
  await c.apply({ kind: 'check_criterion', index: 1 });
  fixture('prepare_complete', c.root, { id: c.seed.id });
  const completed = await c.apply({ kind: 'set_status', status: 'complete' });
  assert.equal(completed.status, 'complete');
  assert.equal((await c.story()).status, 'complete');
});

check('shutdown flushes its answer and exits without waiting for stdin EOF', async t => {
  const c = await setup(t);
  await c.shutdown();
});

check('two writers using one version cannot both acknowledge a content change', async t => {
  const c = await setup(t);
  if (!c.supports('set_fields')) { t.skip('set_fields not declared'); return; }
  const other = connect(t, c.root);
  await other.call('initialize', { protocol: 1, project_root: c.root });
  const before = await c.story();
  const replies = await Promise.all([c, other].map((client, i) => client.rpc('apply', {
    id: before.id, expected_version: before.version, op: { kind: 'set_fields', title: `Parallel writer ${i}` },
  })));
  assert.equal(replies.filter(r => !r.error).length, 1, JSON.stringify(replies));
  error(replies.find(r => r.error), [-32001]);
  assert.equal((await c.story()).title, replies.find(r => !r.error).result.title);
});
