import { render } from 'solid-js/web';
import { SessionCard } from '../../mobile/components/SessionCard';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let selected=[],killed=[];
const session={session_id:'fixture',display_name:'Agent',cwd:'/work/project',worktree_path:null,worktree_branch:null,state:{awaiting_input:false,rate_limited:false,last_activity_ms:Date.now(),agent_intent:'Keep this long intent contained inside its navigation row'}};
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <SessionCard session={session} onSelect={id=>selected.push(id)} onKill={id=>killed.push(id)}/>
  <SessionCard session={{...session,session_id:'child'}} depth={1} onSelect={id=>selected.push(id)}/>
</section>,document.getElementById('root'));
const kill=document.querySelector('[data-testid="kill-btn"]');
const open=document.querySelector('button[data-control-layout="row"]');
kill.click();
if(selected.length||killed.join()!=='fixture')throw new Error('Kill invoked navigation');
open.click();
if(selected.join()!=='fixture'||killed.length!==1)throw new Error('Navigation invoked kill');
const r=kill.getBoundingClientRect(),css=getComputedStyle(kill);
if(r.height+parseFloat(css.marginTop)+parseFloat(css.marginBottom)>28)throw new Error('Kill enlarged card height');
if(innerWidth<=600&&(r.height<44||r.width<44))throw new Error('Kill lost touch target');
if(r.left<open.getBoundingClientRect().right)throw new Error('Kill overlaps navigation');
if(document.querySelectorAll('[data-testid="kill-btn"]').length!==1)throw new Error('Absent kill mounted');
window.controlGeometryReady=true;
