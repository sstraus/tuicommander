import { render } from 'solid-js/web';
import '../../global.css';

// Internal layout reply only: the real designer owns graph edits; no backend is contacted.
window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason);});
window.fetch=async (input,init)=>{
 const path=new URL(typeof input==='string'?input:input.url,location.origin).pathname;
 if(path!=='/workflows/definition/action' || JSON.parse(init.body).action!=='list_drafts')throw new Error(`Unexpected geometry request: ${path}`);
 return new Response(JSON.stringify({type:'drafts',value:[{id:'layout',project:'/layout',name:'Story delivery',kind:'story',draftRevision:1,latestPublishedRevision:0,lastPublishedDraftRevision:0,builtinKey:null,graph:{nodes:[{id:'start',kind:{type:'start'}},{id:'pause',kind:{type:'pause'}},{id:'end',kind:{type:'end'}}],edges:[]}}]}),{headers:{'content-type':'application/json'}});
};
const { WorkflowDesigner }=await import('../../components/WorkflowDesigner/WorkflowDesigner');
render(()=> <section id="geometry" style="width:100%;height:880px;display:flex"><WorkflowDesigner project="/layout" /></section>,document.getElementById('root'));
// Pin consumer density independently of whichever shared classes happen to mount.
window.controlGeometryRows=()=>[
 {size:'compact',heights:[...document.querySelectorAll('#geometry header button')].map(element=>({name:element.textContent.trim(),height:element.getBoundingClientRect().height}))},
 {size:'default',heights:[...document.querySelectorAll('#geometry aside [data-form-control]:not(textarea)')].map(element=>({name:element.getAttribute('aria-label')||element.textContent.trim(),height:element.getBoundingClientRect().height}))},
];
const ready=()=>{
 const pause=document.querySelector('[data-node-id="pause"]');
 if(pause){pause.click();window.controlGeometryReady=true;observer.disconnect();}
};
const observer=new MutationObserver(ready);
observer.observe(document.getElementById('root'),{childList:true,subtree:true});
ready();
