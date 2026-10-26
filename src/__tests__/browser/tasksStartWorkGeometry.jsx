import { render } from 'solid-js/web';
import { StartWorkMenu } from '../../components/Tasks/StartWorkMenu';
import { Select } from '../../components/ui/Select';
import { detail, provider } from '../../components/Tasks/detail/__tests__/detailFixture';
import '../../global.css';

const value = detail();
value.actions = [{id:'start',name:'Start work',kind:'start_work',enabled:true,disabledReason:null}];
const source = provider({detail:async()=>value});
render(() => <section id="geometry" style="padding:16px;min-height:140px;background:var(--bg-secondary)">
  <Select aria-label="Status"><option>Ready</option></Select>
  <StartWorkMenu provider={source} item={value.item} repoPath="/fixture"/>
</section>, document.getElementById('root'));
document.querySelector('[aria-haspopup="menu"]').click();
while (!document.querySelector('[role="menuitem"]')) await new Promise(resolve=>requestAnimationFrame(resolve));
window.controlGeometryReady = true;
