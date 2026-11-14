import { EditorView } from '@codemirror/view';
import { createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { ComposePanel } from '../../components/ComposePanel/ComposePanel';
import CanvasTerminal from '../../components/Terminal/CanvasTerminal';
import { calls, ready } from './composeReservationTransport';
import terminal from '../../components/Terminal/Terminal.module.css';
import '../../global.css';

const [open, setOpen] = createSignal(false);
const [pinned, setPinned] = createSignal(false);
const [chat, setChat] = createSignal(false);
const frame = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const heights = [];
let canvas;
const settle = async () => { await frame(); await new Promise(resolve => setTimeout(resolve, 150)); await frame(); };
render(() => <section style="width:min(900px, calc(100vw - 48px));height:600px;margin:24px;background:var(--bg-primary)">
 <div class={terminal.wrapper}>
  <div class={terminal.content} data-terminal style="background:var(--bg-secondary);overflow:hidden">
   <div style="flex:1;min-height:0"><CanvasTerminal sessionId="compose-proof" terminalId="compose-proof" onRef={ref=>canvas=ref}/></div>
   <pre>Last terminal row</pre>
  </div>
  <Show when={open()}><ComposePanel isOpen={open} initialText={()=>''} reserveMinimum={()=>!chat()}
   onClose={()=>pinned() ? canvas.focus() : setOpen(false)} onDismiss={()=>setOpen(false)} pinned={()=>pinned() || chat()} persistent={chat}
   onTogglePin={()=>setPinned(!pinned())} onSend={()=>{}} onEnqueue={()=>{}} canEnqueue={()=>true}
   queuedCount={()=>0} onClearQueue={()=>{}} onLoadQueue={async()=>[]} onRemoveQueued={()=>{}} focusRequest={()=>0}/></Show>
 </div>
</section>, document.getElementById('root'));
const target = document.querySelector('[data-terminal]');
new ResizeObserver(() => heights.push(target.getBoundingClientRect().height)).observe(target);
window.composeReservation = {
 async ready() { while(!ready || !canvas) await frame(); await settle(); },
 settle,
 async focusEditor() { EditorView.findFromDOM(document.querySelector('.cm-editor')).focus(); await settle(); },
 async open(pin=false, chatMode=false) { setPinned(pin); setChat(chatMode); setOpen(true); await frame(); },
 async close() { setOpen(false); await frame(); },
 async draft(lines) {
  const editor = document.querySelector('.cm-editor');
  const view = EditorView.findFromDOM(editor);
  view.dispatch({changes:{from:0,to:view.state.doc.length,insert:Array(lines).fill('A draft line').join('\n')}});
  await frame();
 },
 read() {
  const editor = document.querySelector('[data-compose-editor]');
  const panel = editor?.closest('[class*="panel"]');
  const box = target.getBoundingClientRect();
  return { terminalHeight:box.height, terminalBottom:box.bottom, editorHeight:editor?.getBoundingClientRect().height, editorTop:editor?.getBoundingClientRect().top,
   panelTop:panel?.getBoundingClientRect().top, panelHeight:panel?.getBoundingClientRect().height, heights:[...heights], resizeCalls:calls.filter(call=>call.cmd==='resize_pty'), draft:document.querySelector('.cm-editor') ? EditorView.findFromDOM(document.querySelector('.cm-editor')).state.doc.toString() : null,
   scroller:editor ? {height:editor.querySelector('.cm-scroller').clientHeight, scroll:editor.querySelector('.cm-scroller').scrollHeight} : null,
   terminalFocused:document.querySelector('[data-terminal-container]').contains(document.activeElement) };
 }
};
