import { render } from 'solid-js/web';
import { SessionHeaderOverlay } from '../../mobile/components/SessionHeaderOverlay';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let closed=0;
render(() => <section id="geometry" style="position:relative;height:420px;padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <SessionHeaderOverlay mode="details" session={{session_id:'geometry',cwd:'/work/project',worktree_path:null,worktree_branch:null}} state={{awaiting_input:false,rate_limited:false,last_activity_ms:0,agent_intent:'Inspect this session'}} onClose={()=>{closed++;}}/>
</section>,document.getElementById('root'));
const button=document.querySelector('button[aria-label="Close session details"]');
button.click();
if(closed!==1)throw new Error('Close callback changed');
const dialog=document.querySelector('[role="dialog"]');
dialog.parentElement.click();
if(closed!==2)throw new Error('Backdrop close changed');
dialog.click();
if(closed!==2)throw new Error('Dialog content closed overlay');
window.controlGeometryReady=true;
