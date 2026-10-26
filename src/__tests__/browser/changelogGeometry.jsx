import { render } from 'solid-js/web';
import { ChangelogModal } from '../../components/ChangelogModal/ChangelogModal';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>({markdown:'# Changes\n\nShared action controls.'})};
render(()=><section id="geometry"><ChangelogModal repoPath="/fixture" onClose={()=>{}}/></section>,document.getElementById('root'));
const ready=()=>{
 const content=document.querySelector('pre');
 if(!content){requestAnimationFrame(ready);return;}
 content.closest('[class*="modal"]').parentElement.style.position='static';
 window.controlGeometryReady=true;
};
requestAnimationFrame(ready);

window.geometryIconLabels=["Close"];
