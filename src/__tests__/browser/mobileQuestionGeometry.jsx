import { render } from 'solid-js/web';
import { QuestionBanner } from '../../mobile/components/QuestionBanner';
import { Button, Select } from '../../components/ui';
import '../../mobile/mobile.css';
let navigated;
const waiting={session_id:'waiting',display_name:'Wiz-Pr',cwd:'/work/project',state:{awaiting_input:true,question_text:'Which implementation should I use? '.repeat(6)}};
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <QuestionBanner sessions={[waiting,{...waiting,session_id:'idle',state:{awaiting_input:false}}]} onNavigate={id=>{navigated=id;}}/>
  <div data-row="default" style="display:flex;gap:8px;margin-top:12px"><Button>Reference action</Button><Select aria-label="Reference status"><option>Ready</option></Select></div>
</section>,document.getElementById('root'));
await Promise.all(document.getAnimations().map(animation=>animation.finished));
const rows=document.querySelectorAll('button[data-control-layout="row"]');
if(rows.length!==1 || rows[0].type!=='button' || rows[0].getBoundingClientRect().height<44)throw new Error('Question row semantics or target changed');
rows[0].click();
if(navigated!=='waiting' || !rows[0].textContent.includes('Wiz-Pr') || !rows[0].textContent.includes('project'))throw new Error('Question navigation or casing changed');
if(rows[0].getBoundingClientRect().right>innerWidth)throw new Error('Long question overflows row');
window.controlGeometryReady=true;
