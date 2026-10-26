import { render } from 'solid-js/web';
import { LastPromptBar } from '../../components/Terminal/LastPromptBar';
import { ViewModeToggle } from '../../components/Terminal/TerminalChatView';
import { terminalsStore } from '../../stores/terminals';
import { Select } from '../../components/ui';
import '../../global.css';

const id=terminalsStore.add({sessionId:'geometry',cwd:null,name:'Geometry',fontSize:14,awaitingInput:null,agentType:'claude'});

render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <ViewModeToggle terminalId={id}/><LastPromptBar intent={()=>"Checking shared terminal controls"} ptyDescription={()=>null} prompt={()=>null}/><Select aria-label="Reference status"><option>Ready</option></Select>
</section>,document.getElementById('root'));
document.querySelector('[role="tablist"]').setAttribute('data-form-control','');
window.controlGeometryReady=true;
