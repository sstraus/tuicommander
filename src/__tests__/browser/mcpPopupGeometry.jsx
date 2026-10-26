import { render } from 'solid-js/web';
import { McpPopup } from '../../components/McpPopup/McpPopup';
import { mcpPopupStore } from '../../stores/mcpPopup';
import { Select } from '../../components/ui/Select';
import '../../global.css';
// Synthetic empty service configuration: exercise the real popup, not backend status.
window.__TAURI_INTERNALS__={invoke:async command=>command==='load_mcp_upstreams'?{servers:[]}:command==='get_mcp_upstream_status'?{upstreams:[]}:null};
mcpPopupStore.open();
render(()=><McpPopup onOpenSettings={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const button=[...document.querySelectorAll('button')].find(button=>button.textContent==='Manage in Settings');
 if(!button)throw new Error('MCP footer did not mount');
 const row=button.parentElement;
 row.id='geometry';
 row.dataset.row='default';
 const reference=document.createElement('div');row.append(reference);
 render(()=><Select aria-label="Geometry reference"><option>Reference</option></Select>,reference);
 row.append(reference.firstElementChild);reference.remove();
 window.controlGeometryReady=true;
});
