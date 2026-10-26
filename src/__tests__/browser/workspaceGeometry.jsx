import { render } from 'solid-js/web';
import '../../global.css';

window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason?.stack??event.reason);});
// Internal layout replies only: no OS hotkey registration, native events or persistence.
window.workspaceHotkeys=[];
window.__TAURI_INTERNALS__={
 transformCallback:()=>1,
 unregisterCallback:()=>{},
 invoke:async (command,args)=>{
  if(command==='set_global_hotkey'){window.workspaceHotkeys.push(args.combo);return;}
  if(command==='plugin:event|listen')return 1;
  if(command==='plugin:event|unlisten')return;
  throw new Error(`Unexpected geometry IPC: ${command}`);
 }
};
const [{KeyboardShortcutsTab},{settingsStore}]=await Promise.all([
 import('../../components/SettingsPanel/tabs/KeyboardShortcutsTab'),import('../../stores/settings')
]);
await settingsStore.setGlobalHotkey('Super+Shift+Y');
render(()=> <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)"><KeyboardShortcutsTab /></section>,document.getElementById('root'));
const search=document.querySelector('input[placeholder="Search shortcuts..."]');
search.value='New terminal tab';
search.dispatchEvent(new Event('input',{bubbles:true}));
window.controlGeometryReady=true;
