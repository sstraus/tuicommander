import { render } from 'solid-js/web';
import { ProgressDialog } from '../../components/ProgressDialog/ProgressDialog';
import { progressStore } from '../../stores/progress';
import '../../global.css';
// Synthetic journal, no persistence: real store controls loading and view selection.
window.__TAURI_INTERNALS__={invoke:async command=>command==='progress_list'?{project:'/fixture',entries:[{id:1,project:'/fixture',createdAtMs:Date.now(),type:'done',text:'A shared control progress entry'}],nextCursor:1,ptyIds:[]}:null};
progressStore.open('/fixture',null);
render(()=><ProgressDialog projects={["/fixture","/other"]}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const select=document.querySelector('select[aria-label="Progress terminal"]');
 if(!select)throw new Error('Progress did not mount');
 select.closest('[class*="dialog"]').id='geometry';
 window.controlGeometryReady=true;
});
