import { render } from 'solid-js/web';
import { IdeasPanel } from '../../components/IdeasPanel/IdeasPanel';
import { ideasStore } from '../../stores/ideas';
import '../../global.css';
// Synthetic notes hydrate only; edit exposes the image-removal action without saving.
const image='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="80" height="60"><rect width="80" height="60" fill="gray"/></svg>');
window.__TAURI_INTERNALS__={invoke:async command=>command==='load_notes'?{notes:[{id:'geometry-note',text:'Geometry idea',images:['/fixture/image.png'],repoPath:null,repoDisplayName:null,createdAt:Date.now(),usedAt:null}]}:undefined,transformCallback:()=>0,convertFileSrc:()=>image};
await ideasStore.hydrate();
render(()=><IdeasPanel visible repoPath={null} mode="detached" onClose={()=>{}} onSendToTerminal={()=>{}} onQueueToTerminal={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const panel=document.querySelector('#notes-panel');panel.id='geometry';panel.style.width='390px';panel.style.maxWidth='100%';panel.style.height='600px';
 panel.querySelector('button[title="Edit idea"]').click();
 window.controlGeometryReady=true;
});
