import { render } from 'solid-js/web';
import { FileBrowserPanel } from '../../components/FileBrowserPanel/FileBrowserPanel';
import { uiStore } from '../../stores/ui';
import '../../global.css';
// No repository is selected: no filesystem read, mutation, watcher or drag/drop action is invoked.
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
await uiStore.hydrate();
render(()=><section id="geometry"><FileBrowserPanel visible={true} repoPath={null} mode="detached" onClose={()=>{}} onFileOpen={()=>{}}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 document.querySelector('button[title="Switch to content search"]').click();
 window.controlGeometryReady=true;
});
