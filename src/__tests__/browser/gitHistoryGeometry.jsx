import { render } from 'solid-js/web';
import { HistoryTab } from '../../components/GitPanel/HistoryTab';
import { Select } from '../../components/ui/Select';
import '../../global.css';
// Synthetic service state: this harness tests real UI geometry, not git or transport.
const commits=Array.from({length:50},(_,i)=>({hash:`hash${i}`,subject:`Commit ${i}`,author_name:'Fixture',author_date:'2026-10-24T12:00:00Z',parents:[],refs:[]}));
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_file_history'?commits:[]};
render(() => <section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><HistoryTab repoPath="/fixture" filePath="file.ts" onOpenDiff={()=>{}}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const button=[...document.querySelectorAll('button')].find(button=>button.textContent==='Load more');
 if(!button)throw new Error('History pagination did not mount');
 const row=button.parentElement;
 row.dataset.row='default';
 const reference=document.createElement('div');
 row.append(reference);
 render(()=><Select aria-label="Geometry reference"><option>Reference</option></Select>,reference);
 row.append(reference.firstElementChild);
 reference.remove();
 // Screenshot only the real pagination row and shared reference, not all 50 commits.
 document.querySelector('#geometry').removeAttribute('id');
 row.id='geometry';
 window.controlGeometryReady=true;
});
