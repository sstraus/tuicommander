import { render } from 'solid-js/web';
import { Terminal } from '../../components/Terminal/Terminal';
import { terminalsStore } from '../../stores/terminals';
import { Select } from '../../components/ui';
import '../../global.css';

// A suspended tab deliberately has no PTY; only its production presentation is mounted.
const id = terminalsStore.add({sessionId:null,cwd:null,name:'Geometry',fontSize:14,awaitingInput:null});
terminalsStore.update(id,{suspended:true,pendingResumeCommand:'resume'});
render(() => <section id="geometry" style="height:300px;padding:16px;background:var(--bg-secondary)">
  <Terminal id={id} alwaysVisible/>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>, document.getElementById('root'));
window.controlGeometryReady = true;
