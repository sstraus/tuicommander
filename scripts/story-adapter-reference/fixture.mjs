#!/usr/bin/env node
// Store-side fixture for the selectable protocol conformance suite.
import { readFileSync, writeFileSync, renameSync } from 'node:fs';
import { withStoreLock } from './store-lock.mjs';
import { resolve } from 'node:path';

const [action, root, input = '{}'] = process.argv.slice(2);
const fields = JSON.parse(input);
const file = resolve(root, '.tuic-reference-stories.json');
const load = () => JSON.parse(readFileSync(file, 'utf8'));
const save = db => {
  const tmp = `${file}.fixture.${process.pid}.tmp`;
  writeFileSync(tmp, JSON.stringify(db));
  renameSync(tmp, file);
};
const seed = (db, values = {}) => {
  const story = {
    id: `s${db.stories.length + 1}`, plan_id: 'default', title: 'Seed story',
    status: 'pending', priority: 2, problem: 'A persisted story for protocol conformance',
    criteria: [{ text: 'first criterion', checked: false }, { text: 'second criterion', checked: false }],
    dependencies: [], notes: [], ...values,
  };
  story.changed_seq = ++db.seq;
  db.stories.push(story);
  save(db);
  return { id: story.id, plan_id: story.plan_id };
};
const result = withStoreLock(file, () => {
  let result = {};
  if (action === 'setup') {
    result = seed({ seq: 0, plans: [{ id: 'default', title: 'Default' }], stories: [] });
  } else if (action === 'create') {
    result = seed(load(), fields);
  } else if (action === 'change') {
    const db = load();
    const story = db.stories.find(s => s.id === fields.id);
    if (!story) throw new Error(`missing fixture story ${fields.id}`);
    story.title = fields.title;
    story.changed_seq = ++db.seq;
    save(db);
  } else if (action !== 'prepare_complete') {
    throw new Error(`unknown fixture action ${action}`);
  }
  return result;
});
process.stdout.write(`${JSON.stringify(result)}\n`);
