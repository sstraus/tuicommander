import { render } from 'solid-js/web';
import { SmartPromptsTab } from '../../components/SettingsPanel/tabs/SmartPromptsTab';
import { SMART_PROMPTS_BUILTIN } from '../../data/smartPromptsBuiltIn';
import { promptLibraryStore } from '../../stores/promptLibrary';
import '../../global.css';

// Internal prompt owner state only: no backend, agent detection or persistence.
window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason);});
window.fetch=async input=>{
 const path=new URL(typeof input==='string'?input:input.url,location.origin).pathname;
 let value;
 if(path==='/config/prompt-library')value={prompts:SMART_PROMPTS_BUILTIN.map(prompt=>({id:prompt.id,label:prompt.name,text:JSON.stringify({...prompt,content:'Fixture content'}),pinned:false}))};
 else if(path==='/agents/detect-all')value={};
 else throw new Error(`Unexpected geometry request: ${path}`);
 return new Response(JSON.stringify(value),{headers:{'content-type':'application/json'}});
};
await promptLibraryStore.hydrate();
promptLibraryStore._testCancelPendingSave();
render(()=> <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)"><SmartPromptsTab /></section>,document.getElementById('root'));
const first=SMART_PROMPTS_BUILTIN[0];
document.querySelector(`button[aria-label="Edit ${first.name}"]`).click();
// Keep the actual editor and popup visible without pages of unrelated library rows.
document.querySelectorAll('#geometry div[role="button"]').forEach(header=>header.click());
[...document.querySelectorAll('#geometry button')].find(button=>button.textContent.includes('Insert variable...')).click();
if(!document.querySelector('#geometry button[data-control-layout="row"]'))throw new Error('Variable rows did not mount');
if(![...document.querySelectorAll('#geometry button')].some(button=>button.textContent==='Reset to Default'))throw new Error('Reset action did not mount');
window.controlGeometryReady=true;
