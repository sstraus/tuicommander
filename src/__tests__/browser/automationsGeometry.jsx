import { render } from 'solid-js/web';
import { AutomationsDialog } from '../../components/AutomationsDialog/AutomationsDialog';
import { fakeAdapter } from '../automationsFixtures';
import '../../global.css';
// Real dialog, in-memory adapter; no scheduling, session launch or worktree mutation.
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
render(()=><AutomationsDialog adapter={fakeAdapter()} onClose={()=>{}}/>,document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
for(let i=0;i<180&&!document.querySelector('button[data-control-layout="card"]');i++)await frame();
if(!document.querySelector('button[data-control-layout="card"]'))throw new Error('Automation card did not mount');
for(let i=0;i<180&&![...document.querySelectorAll('button')].some(b=>b.textContent==='Save');i++)await frame();
if(![...document.querySelectorAll('button')].some(b=>b.textContent==='Save'))throw new Error('Automation editor did not mount');
document.querySelector('[role="dialog"]').id='geometry';
window.controlGeometryReady=true;
