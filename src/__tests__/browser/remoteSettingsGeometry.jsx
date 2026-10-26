import { render } from 'solid-js/web';
import { RemoteMachinesPanel } from '../../components/SettingsPanel/tabs/services/RemoteMachinesPanel';
import { remoteConnectionsStore } from '../../stores/remoteConnections';
import '../../global.css';

// Renderer-only owner boundary: no SSH host is contacted and no app instance is used.
const connection = (id, name, outOfDate) => ({
  connection: {id, name, transport: {type:'Ssh',ssh_host:id,ssh_port:22,ssh_user:'dev',identity_file:null,remote_daemon_port:9876},auth_username:'',enabled:true,deploy:'never',survive_secs:1800},
  status:'connected', outOfDate,
});
remoteConnectionsStore.hydrate = async () => {};
remoteConnectionsStore.getConnections = () => ({current:connection('current','Current build',false),older:connection('older','Older build',true)});
remoteConnectionsStore.discoverSshHosts = async () => ({hosts:[{host:'build-host',target:'build-host',user:'dev',port:22,source:'config'},{host:'test-host',target:'test-host',user:null,port:null,source:'known_hosts'}],hashed_count:0});
remoteConnectionsStore.sshAgentInfo = async () => ({keys:[{fingerprint:'fixture',comment:'Fixture key',key_type:'ED25519'}],agent_type:'Fixture'});
render(() => <section id="geometry" style="width:min(1100px,100%);padding:16px;background:var(--bg-secondary)"><h2>Connections and SSH discovery</h2><RemoteMachinesPanel/></section>, document.getElementById('root'));
window.controlGeometryRows = () => {
  const probes=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()==='Probe');
  if(probes.length!==2) throw new Error('Expected both discovered hosts');
  const gaps=probes.map(probe=>{
    const host=probe.previousElementSibling.getBoundingClientRect();
    const action=probe.getBoundingClientRect();
    const gap=action.top>=host.bottom? action.top-host.bottom:action.left-host.right;
    if(Math.abs(gap-20)>0.5) throw new Error(`Discovery row lost shared 20px gap: ${gap}`);
    return gap;
  });
  const updates=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()==='Update & restart remote');
  const warnings=[...document.querySelectorAll('span')].filter(b=>b.textContent==='Remote out of date');
  if(updates.length!==1||warnings.length!==1) throw new Error('Current build must hide both warning and update action');
  if(!updates[0].parentElement.textContent.includes('Older build')) throw new Error('Only older build offers update');
  return [{size:'default',gaps,heights:probes.map(button=>({tag:'BUTTON',name:'Probe',height:button.getBoundingClientRect().height}))}];
};
requestAnimationFrame(function ready(){
  if([...document.querySelectorAll('button')].filter(b=>b.textContent.trim()==='Probe').length===2) window.controlGeometryReady=true;
  else requestAnimationFrame(ready);
});
