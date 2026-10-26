import { render } from 'solid-js/web';
import { MarkdownPanel } from '../../components/MarkdownPanel/MarkdownPanel';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
render(()=><section id="geometry"><MarkdownPanel visible repoPath={null} mode="detached" onClose={()=>{}}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const input=document.querySelector('#geometry input');input.value='*.md';input.dispatchEvent(new Event('input',{bubbles:true}));
 document.querySelector('button[title="Switch to content search"]').click();
 window.controlGeometryReady=true;
});
