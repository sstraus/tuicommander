import { render } from 'solid-js/web';
import { ConnectionBanner } from '../../mobile/components/ConnectionBanner';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let retried=0;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <ConnectionBanner offline authError={false} onRetry={()=>{retried++;}}/>
  <ConnectionBanner offline authError onRetry={()=>{throw new Error('Auth retry must not appear');}}/>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>, document.getElementById('root'));
document.querySelector('button').click();
if(retried!==1 || !document.querySelector('a[href^="/mobile/login?next="]'))throw new Error('Connection actions changed');
window.controlGeometryReady=true;
