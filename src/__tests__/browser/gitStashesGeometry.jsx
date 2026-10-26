import { render } from 'solid-js/web';
import { StashesTab } from '../../components/GitPanel/StashesTab';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_stash_list'?[{index:0,ref_name:'stash@{0}',message:'Geometry fixture',hash:'abc'}]:[]};
render(()=><section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><StashesTab repoPath="/fixture"/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const apply=[...document.querySelectorAll('button')].find(button=>button.textContent==='Apply');
 if(!apply)throw new Error('Stash actions did not mount');
 apply.parentElement.dataset.row='compact';
 window.controlGeometryReady=true;
});
