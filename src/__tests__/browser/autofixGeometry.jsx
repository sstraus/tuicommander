import { render } from 'solid-js/web';
import { AutofixDialog } from '../../components/AutofixDialog/AutofixDialog';
import '../../global.css';
// Backend IO is not under test; mount the real dialog and editor with its wire shape.
window.__TAURI_INTERNALS__={invoke:async()=>({number:42,title:'Geometry fixture',autofix_prompt:'Investigate the issue and propose a fix.'})};
render(()=><section id="geometry"><AutofixDialog repoPath="/fixture" issueNumber={42} onConfirm={()=>{}} onClose={()=>{}}/></section>,document.getElementById('root'));
const ready=()=>{
 const editor=document.querySelector('textarea');
 if(!editor){requestAnimationFrame(ready);return;}
 const overlay=editor.closest('[class*="modal"]').parentElement;
 overlay.style.position='static';
 window.controlGeometryReady=true;
};
requestAnimationFrame(ready);

window.geometryIconLabels=["Close"];
