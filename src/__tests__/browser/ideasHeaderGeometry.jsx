import { render } from 'solid-js/web';
import { IdeasPanel } from '../../components/IdeasPanel/IdeasPanel';
import { ideasStore } from '../../stores/ideas';
import '../../global.css';
// Actual panel; a completed synthetic note exposes the clear action. No persistence is exercised.
window.__TAURI_INTERNALS__={invoke:async command=>command==='load_notes'?{notes:[{id:'header-note',text:'Completed idea',images:[],repoPath:null,repoDisplayName:null,createdAt:1,usedAt:2}]}:undefined,transformCallback:()=>0};
await ideasStore.hydrate();
render(()=><IdeasPanel visible repoPath={null} mode="detached" onClose={()=>{}} onSendToTerminal={()=>{}} onQueueToTerminal={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const panel=document.getElementById('notes-panel');panel.id='geometry';panel.style.width='390px';panel.style.maxWidth='100%';panel.style.height='600px';
 if(!panel.querySelector('[aria-label="Clear completed ideas"]'))throw new Error('Clear action did not mount');
 window.controlGeometryReady=true;
});
