import { render } from 'solid-js/web';
import { ComposePanel } from '../../components/ComposePanel/ComposePanel';
import { Composer } from '../../components/AIChatPanel/Composer';
import { Transcript } from '../../components/AIChatPanel/Transcript';
import '../../global.css';

// Render production transcript bubbles without connecting to a live session.
const params = new URLSearchParams(location.search);
const count = Number(params.get('count') || 1);
const mode = params.get('mode') || 'Pending';
const text = Array.from({ length: count }, (_, i) => `[image: /fixtures/diagram-${i + 1}.svg]`).join('\n') + '\nReview these diagrams before the next step.';
const done = async () => {};
const chat = {
  sessionId: () => 'preview-fixture', phase: () => 'live', busy: () => true,
  configOptions: () => [], capabilities: () => ({}), operatingRepo: () => '/fixtures', pinnedRepo: () => null,
  queuedPrompts: () => [{ turnId: 'preview', summary: text }],
  cancelQueued: done, cancel: done, send: done, toggleRepoPin: () => {},
};
render(() => <main style="padding:32px;max-width:840px;margin:auto;height:90vh;display:flex;flex-direction:column">
  <h2>{mode} · {count} attached {count === 1 ? 'image' : 'images'}</h2>
  {mode === 'Compose queue' ? <ComposePanel
    isOpen={() => true} initialText={() => ''} onClose={done} onSend={done} onEnqueue={done}
    canEnqueue={() => true} queuedCount={() => 1} onClearQueue={done}
    onLoadQueue={async () => [{ id: 1, kind: 'user_command', text }]} onRemoveQueued={done}
    pinned={() => true} onTogglePin={done} onDismiss={done} focusRequest={() => 0}
  /> : mode === 'AI queue' ? <Composer chat={chat} /> : <Transcript entries={() => [{ id: 'images', kind: 'user', text, pending: mode === 'Pending' }]}
    busy={() => false} emptyMessage="Empty" terminalAgent={mode === 'AI Chat' ? undefined : 'claude'} />}
</main>, document.getElementById('root'));
if (mode === 'Compose queue') requestAnimationFrame(() => [...document.querySelectorAll('button')].find(button => button.textContent.includes('1 queued'))?.click());
// Check native image loading and physical frame geometry in a real browser.
const verify = () => {
  const images = [...document.querySelectorAll('img')];
  if (images.length !== count || images.some(image => !image.complete || !image.naturalWidth)) return;
  const boxes = images.map(image => image.getBoundingClientRect());
  const valid = count === 1 || (Math.abs(boxes[0].top - boxes[1].top) < 1 && boxes[1].left > boxes[0].right && (count < 3 || boxes[2].top > boxes[0].bottom));
  document.documentElement.dataset.layoutVerified = String(valid);
};
document.addEventListener('load', verify, true);
requestAnimationFrame(verify);
