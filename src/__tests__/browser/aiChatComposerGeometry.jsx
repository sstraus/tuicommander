import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import '../../global.css';
import { Composer } from '../../components/AIChatPanel/Composer';
import { aiChatDraft } from '../../components/AIChatPanel/draft';
import s from '../../components/AIChatPanel/AIChatPanel.module.css';

// Real renderer, local draft state; no connection to a running ego or TUIC instance.
const mobile = new URLSearchParams(location.search).has('mobile');
const [busy, setBusy] = createSignal(false);
const chat = {
  sessionId: () => 'composer-alignment', connectionId: () => null,
  phase: () => 'live', busy, queuedPrompts: () => [],
  operatingRepo: () => '/geometry/orchestrator', pinnedRepo: () => null,
  configOptions: () => [], models: () => null, modes: () => null,
  usage: () => ({ used: 20000, size: 100000 }),
  toggleRepoPin() {}, send() {},
};
aiChatDraft.activate('composer-alignment');
aiChatDraft.clear();
render(() => <div id="geometry" style={{ width: mobile ? '390px' : '500px', height: '700px', display: 'flex', 'flex-direction': 'column', margin: '16px' }}><div style={{ flex: 1 }} /><Composer chat={chat} mobileAttachments={mobile} /></div>, document.getElementById('root'));
const box = element => {
  const { top, bottom, left, right, width, height } = element.getBoundingClientRect();
  return { top, bottom, left, right, width, height };
};
const find = name => document.querySelector('.' + s[name].split(' ').join('.'));
window.composerSample = async (text, focus, running = false) => {
  setBusy(running);
  const editor = document.querySelector('textarea');
  aiChatDraft.set(text);
  if (focus) editor.focus(); else editor.blur();
  // The production editor animates height for 120ms; sample the settled geometry.
  await new Promise(resolve => setTimeout(resolve, 180));
  const send = document.querySelector('button[aria-label="Send"],button[aria-label="Queue"]');
  const style = getComputedStyle(editor);
  return {
    text, focus, actualFocus: document.activeElement === editor, focusVisible: editor.matches(':focus-visible'), mobile, busy: running, editor: box(editor), send: box(send), stop: document.querySelector('button[title="Stop"]') ? box(document.querySelector('button[title="Stop"]')) : null,
    header: mobile ? null : box(find('composerHeader')), footer: box(find('composerMeta')),
    focusStyle: { outline: style.outlineStyle, shadow: style.boxShadow, border: style.borderColor },
    input: box(find('inputArea')), body: box(find('inputBody')),
  };
};
window.controlGeometryReady = true;
