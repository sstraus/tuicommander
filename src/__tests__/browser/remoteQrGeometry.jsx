import { render } from 'solid-js/web';
import { RemoteQrDialog } from '../../components/RemoteQrDialog/RemoteQrDialog';
import '../../global.css';
// Synthetic URL/IP response: no live pairing token or external service.
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_local_ips'?[{ip:'192.0.2.1',label:'LAN'},{ip:'192.0.2.2',label:'Other'}]:command==='get_connect_url'?'http://192.0.2.1:9876/connect?token='+ 'fixture'.repeat(20):{services:{server:{enabled:true}}}};
render(()=><RemoteQrDialog onClose={()=>{}}/>,document.getElementById('root'));
const ready=()=>{const button=document.querySelector('button[aria-label="Copy connection URL"]');if(!button)return requestAnimationFrame(ready);button.closest('[class*="dialog"]').id='geometry';window.controlGeometryReady=true;};requestAnimationFrame(ready);
