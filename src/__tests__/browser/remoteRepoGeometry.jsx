import { render } from 'solid-js/web';
import { RemoteRepoPicker } from '../../components/RemoteRepoPicker/RemoteRepoPicker';
import '../../global.css';
// Frontend-only listing fixture; no real remote host or registration.
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_home_directory'?'/fixture':[{name:'project',is_dir:true,path:'/fixture/project',size:0,modified_at:0,git_status:'',is_ignored:false}]};
render(()=><RemoteRepoPicker visible connectionName="Fixture host" onClose={()=>{}} onConfirm={()=>{}}/>,document.getElementById('root'));
const ready=()=>{const row=[...document.querySelectorAll('button[data-control-layout="row"]')].find(b=>b.textContent==='project');if(!row)return requestAnimationFrame(ready);row.closest('[class*="popover"]').id='geometry';window.controlGeometryReady=true;};requestAnimationFrame(ready);
