import { render } from 'solid-js/web';
import { TunnelsPanel } from '../../components/TunnelsPanel/TunnelsPanel';
import { tunnelPanelStore } from '../../stores/tunnelPanel';
import '../../global.css';
// Synthetic profiles only: no SSH launch or network operation.
const profile={id:'fixture',name:'Fixture tunnel',host:'host.example',port:22,user:'dev',identity_file:null,forwards:[],options:{server_alive_interval:30,server_alive_count_max:3,strict_host_key_checking:'Yes',compression:true},auto_connect:false};
window.__TAURI_INTERNALS__={invoke:async command=>command==='list_tunnel_profiles'?[profile]:[]};
tunnelPanelStore.open();render(()=><TunnelsPanel/>,document.getElementById('root'));
const ready=()=>{const button=document.querySelector('button[title="Start"]');if(!button)return requestAnimationFrame(ready);button.closest('[class*="dashboard"]').id='geometry';window.controlGeometryReady=true;};requestAnimationFrame(ready);
