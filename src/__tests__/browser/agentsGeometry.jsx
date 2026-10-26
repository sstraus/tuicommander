import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { createAgentConfigsStore } from '../../stores/agentConfigs';
import { AgentConfigProvider } from '../../components/SettingsPanel/tabs/agentConfigContext';
import { LaunchInstructionsEditor } from '../../components/SettingsPanel/tabs/LaunchInstructionsEditor';
import { LaunchModuleSettings } from '../../components/SettingsPanel/tabs/LaunchModuleSettings';
import a from '../../components/SettingsPanel/tabs/AgentsTab.module.css';
import '../../global.css';
// Real agent-settings composites with an in-memory internal store: no external installation or persistence.
async function mount() {
 const store=createAgentConfigsStore({load:async()=>({agents:{},launch_modules:{waiting:false},launch_instructions:'Fixture additions'}),save:async()=>{}});
 await store.hydrate();
 const [value,setValue]=createSignal('Fixture additions');
 render(()=> <section id="geometry" class={a.page} style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
  <AgentConfigProvider value={store}><LaunchModuleSettings agentType="ego"/></AgentConfigProvider>
  <LaunchInstructionsEditor id="custom" settingId="agents.launch_instructions" label="Launch additions" value={value()} fallback="Generated modules" inheritanceLabel="Generated modules" customizeLabel="Add instructions" resetLabel="Clear additions" requiredMarkers={[]} onSave={setValue}/>
  <LaunchInstructionsEditor id="inherited" settingId="agents.launch_instructions" label="Inherited additions" value={undefined} fallback="Generated modules" inheritanceLabel="Generated modules" customizeLabel="Add instructions" resetLabel="Clear additions" requiredMarkers={[]} onSave={()=>{}}/>
 </section>,document.getElementById('root'));
 requestAnimationFrame(()=>{window.controlGeometryReady=true;});
}
mount();
