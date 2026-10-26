import { render } from 'solid-js/web';
import { IdeasOverlay } from '../../mobile/components/IdeasOverlay';
import { ideasStore } from '../../stores/ideas';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';

// Internal HTTP fixture only: never read or mutate the running app's notes.
const fetchOriginal = window.fetch.bind(window);
window.fetch = (input, init) => {
  const url = typeof input === 'string' ? input : input.url;
  if (url.includes('/config/notes')) return Promise.resolve(Response.json({notes:[{id:'geometry-note',text:'A long idea that wraps beside the send and delete actions. '.repeat(8),createdAt:Date.now(),repoPath:null,repoDisplayName:null,usedAt:null,images:[]}]}));
  return fetchOriginal(input, init);
};
await ideasStore.hydrate();
let dismissed=0;
render(() => <section id="geometry" style="position:fixed;inset:0;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <IdeasOverlay sessionId="geometry-only" repoPath={null} onDismiss={()=>{dismissed++;}}/>
</section>, document.getElementById('root'));
const input=document.querySelector('textarea');
const add=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='+');
if(!add?.disabled) throw new Error('Empty idea must disable add');
input.value='Keep this draft';
input.dispatchEvent(new Event('input',{bubbles:true}));
if(add.disabled)throw new Error('Idea input must enable add');
const shift=new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true,cancelable:true});
input.dispatchEvent(shift);
if(shift.defaultPrevented)throw new Error('Shift Enter must retain native multiline editing');
input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
if(dismissed!==1)throw new Error('Escape must dismiss');
if(!document.querySelector('button[title="Send to terminal"]') || !document.querySelector('button[title="Delete"]'))throw new Error('Idea row actions missing');
const minimum=innerWidth<=600?44:36;
if(input.getBoundingClientRect().height!==minimum)throw new Error('Idea textarea must use shared auto-grow minimum');
if(input.getBoundingClientRect().right>innerWidth)throw new Error('Idea editor overflow');
window.controlGeometryReady=true;
