import { render } from 'solid-js/web';
import { ChoicePromptOverlay } from '../../mobile/components/ChoicePromptOverlay';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
const answers=[];
const prompt={title:'Choose an answer',selection_mode:'toggle-enter',options:[{key:'1',label:'A long answer that may wrap when the viewport is narrow',highlighted:true},{key:'2',label:'No',destructive:true,hint:'Escape'}]};
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <ChoicePromptOverlay embedded prompt={prompt} onSelect={key=>answers.push(key)}/>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>, document.getElementById('root'));
const rows=[...document.querySelectorAll('button[data-control-layout="row"]')];
rows[0].click();document.querySelector('button[data-form-control]').click();
if(answers.join('|')!=='1|\r')throw new Error('Choice key or submit callback changed');
window.controlGeometryReady=true;
