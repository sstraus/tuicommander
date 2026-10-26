import { render } from 'solid-js/web';
import { ChangesTab } from '../../components/GitPanel/ChangesTab';
import '../../global.css';
// Synthetic service data only; the real component and shared controls own interaction/layout.
const status={staged:[{path:'staged.ts',status:'M',additions:1,deletions:0}],unstaged:[{path:'edited.ts',status:'M',additions:1,deletions:0}],untracked:[]};
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_working_tree_status'?status:[]};
render(()=><section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><ChangesTab repoPath="/fixture" onOpenDiff={()=>{}}/></section>,document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
const replace=value=>window.dispatchEvent(new CustomEvent('smart-prompt:commit-message',{detail:value}));
requestAnimationFrame(async()=>{
 const field=document.querySelector('textarea');
 const filter=document.querySelector('input[placeholder^="Filter"]');
 if(!field||!filter)throw new Error('Changes controls did not mount');
 filter.value='*';filter.dispatchEvent(new InputEvent('input',{bubbles:true}));
 await frame();
 filter.parentElement.dataset.row='default';
 // Real file action row: measure its compact controls separately from text layout.
 document.querySelector('[title="Stage"]').parentElement.dataset.row='compact';
 replace('Initial\nmultiline\ndraft');
 await frame();
 window.textareaGeometry=async()=>{
  const read=()=>({height:field.getBoundingClientRect().height,scroll:field.scrollHeight,client:field.clientHeight,overflow:getComputedStyle(field).overflowY});
  const initial=read();
  field.value=Array(30).fill('Long commit message').join('\n');field.dispatchEvent(new InputEvent('input',{bubbles:true}));await frame();const long=read();
  field.value='';field.dispatchEvent(new InputEvent('input',{bubbles:true}));await frame();const cleared=read();
  replace('one\ntwo\nthree');await frame();const replaced=read();
  replace('A commit message with enough words to wrap across narrow widths.');await frame();const wide=read();
  field.style.flex='none';field.style.width='100px';await frame();const narrow=read();
  field.style.removeProperty('flex');field.style.removeProperty('width');replace('Geometry fixture');await frame();
  return {initial,long,cleared,replaced,wide,narrow};
 };
 window.controlGeometryReady=true;
});
