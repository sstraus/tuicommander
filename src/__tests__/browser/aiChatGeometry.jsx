import { render } from 'solid-js/web';
import '../../global.css';

// Real consumer components with in-memory chat props; no ego launch, deletion or IPC proof.
window.__TAURI_INTERNALS__ = { invoke: async () => undefined };
const [{ SessionControls }, { Composer }, { Transcript }, { Interactions }, { aiChatDraft }, { default: s }] = await Promise.all([
  import('../../components/AIChatPanel/SessionControls'),
  import('../../components/AIChatPanel/Composer'),
  import('../../components/AIChatPanel/Transcript'),
  import('../../components/AIChatPanel/Interactions'),
  import('../../components/AIChatPanel/draft'),
  import('../../components/AIChatPanel/AIChatPanel.module.css'),
]);
const done = async () => {};
const chat = {
  sessionId: () => 'geometry-chat', title: () => 'A long conversation title that must not squeeze shared controls',
  phase: () => 'live', busy: () => true, held: () => false,
  capabilities: () => ({ egoHoldVersion: 1, egoCompactVersion: 1, fork: true, delete: true }),
  sessions: () => [{ sessionId: 'geometry-chat', title: 'Geometry conversation' }],
  configOptions: () => [{ id: 'model', name: 'Model', type: 'select', currentValue: 'fixture', options: [{ value: 'fixture', name: 'Fixture model' }] }],
  queuedPrompts: () => [{ turnId: 'queued', summary: 'Check the mounted geometry' }],
  operatingRepo: () => '/geometry/repository', pinnedRepo: () => null,
  launch: () => null, error: () => null, clearError: () => {},
  startSession: done, selectSession: done, setOption: done, pause: done, resume: done, compact: done,
  fork: done, deleteSession: done, cancelQueued: done, cancel: done, send: done, toggleRepoPin: () => {},
};
const entries = [
  { id: 'user', kind: 'user', text: 'Show shared controls' },
  { id: 'reply', kind: 'agent', text: 'A reply with fenced code.\n\n```sh\necho geometry\n```', messageId: 'reply-message' },
  { id: 'notice', kind: 'notice', text: 'Review the result', action: { kind: 'open_result', path: '/geometry/result.md' } },
];
const interactions = [
  { kind: 'permission', requestId: 'permission', request: { toolCall: { title: 'Permission geometry' }, options: [{ optionId: 'allow', name: 'Allow once', kind: 'allow_once' }, { optionId: 'deny', name: 'Deny', kind: 'reject_once' }] } },
  { kind: 'elicitation', requestId: 'form', request: { mode: 'form', message: 'Form geometry', requestedSchema: { type: 'object', properties: { name: { type: 'string', title: 'Name' }, amount: { type: 'number', title: 'Amount' }, enabled: { type: 'boolean', title: 'Enabled' }, choice: { type: 'string', title: 'Choice', enum: ['one', 'two'] } } } } },
];
aiChatDraft.activate('geometry-chat');
aiChatDraft.set('A complete composing line');
aiChatDraft.stageFile({ name: 'notes.md', path: '/geometry/notes.md' });
const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=';
const image = new File([Uint8Array.from(atob(png), c => c.charCodeAt(0))], 'image.png', { type: 'image/png' });
await aiChatDraft.stageImage(image, true);
let search;
render(() => <div id="geometry" style={{ width: 'min(500px, calc(100vw - 32px))', margin: '16px', 'background-color': 'var(--bg-primary)' }}>
  <div class={s.tidyHeader}><SessionControls chat={chat} onNewOptions={() => {}} /></div>
  <div style={{ height: '340px', display: 'flex', 'flex-direction': 'column' }}>
    <Transcript entries={() => entries} busy={() => false} emptyMessage="" onSearchRef={ref => { search = ref; }} canForkAtMessage={() => true} onFork={() => {}} />
  </div>
  <Interactions interactions={() => interactions} onPermission={() => {}} onPermissionDismissed={() => {}} onElicitationAccepted={() => {}} onElicitationDeclined={() => {}} onElicitationCancelled={() => {}} />
  <div data-composer-frame style={{ height: '400px', display: 'flex', 'flex-direction': 'column' }}><Composer chat={chat} /></div>
</div>, document.getElementById('root'));
search.open();
const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
await frame(); await frame();
for (const details of document.querySelectorAll('#geometry details')) details.open = true;
window.geometryIconLabels = ['Delete Geometry conversation', 'Close chat search', 'Cancel queued prompt Check the mounted geometry', 'Remove pasted image', 'Remove notes.md', 'Park draft', 'Queue'];
window.aiChatGeometry = () => {
  const box = element => { if (!element) throw new Error('Missing AI Chat geometry target'); const r = element.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, width: r.width, height: r.height }; };
  const picker = document.querySelector('[data-control-layout="overlay-picker"]');
  return {
    picker: box(picker), visible: box(picker.previousElementSibling),
    header: box(document.querySelector('.' + s.tidyHeader.split(' ').join('.'))),
    summaries: [...document.querySelectorAll('summary[data-control-layout="disclosure"]')].map(box),
    editor: box(document.querySelector('textarea')), editorText: document.querySelector('textarea').value,
    rawActions: [...document.querySelectorAll('#geometry button, #geometry input:not([type="checkbox"]), #geometry select:not([data-control-layout="overlay-picker"]), #geometry textarea')].filter(element => !element.hasAttribute('data-form-control')).map(element => element.outerHTML),
  };
};
window.controlGeometryReady = true;
