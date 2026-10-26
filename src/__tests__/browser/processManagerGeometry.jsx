import { render } from 'solid-js/web';
import { ProcessManagerModal } from '../../components/ProcessManagerModal/ProcessManagerModal';
import { Select } from '../../components/ui/Select';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>[]};
render(()=><ProcessManagerModal onClose={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const button=document.querySelector('button[aria-label="Close"]');
 if(!button)throw new Error('Process manager close did not mount');
 const row=document.createElement('div');row.id='geometry';row.dataset.row='default';row.style='display:flex;align-items:center;gap:8px';
 button.parentElement.append(row);row.append(button);
 const reference=document.createElement('div');row.append(reference);
 render(()=><Select aria-label="Geometry reference"><option>Reference</option></Select>,reference);
 row.append(reference.firstElementChild);reference.remove();
 window.controlGeometryReady=true;
});
