import { render } from 'solid-js/web';
import { BottomTabs } from '../../mobile/components/BottomTabs';
import { Button, Select } from '../../components/ui';
import '../../mobile/mobile.css';
let selected;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <div data-stretch-case="55"><BottomTabs active="sessions" aiChatAvailable onSelect={value=>{selected=value;}}/></div>
  <div data-row="default" style="display:flex;gap:8px"><Button>Reference action</Button><Select aria-label="Reference status"><option>Ready</option></Select></div>
</section>, document.getElementById('root'));
const actions=[...document.querySelector('nav').querySelectorAll('button')];
if(actions.length!==5 || actions[0].getAttribute('aria-current')!=='page')throw new Error('Navigation order or current page changed');
actions[1].click();
if(selected!=='chat')throw new Error('Navigation callback changed');
window.controlGeometryReady=true;
