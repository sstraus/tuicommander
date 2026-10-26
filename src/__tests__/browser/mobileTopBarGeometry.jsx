import { render } from 'solid-js/web';
import { TopBar } from '../../mobile/components/TopBar';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let settings=0;
render(() => <section id="geometry" style="padding:16px;min-height:240px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <TopBar isConnected={false} onOpenSettings={()=>{settings++;}}/>
</section>,document.getElementById('root'));
const more=document.querySelector('[aria-label="More options"][aria-haspopup]');
more.click();
if(more.getAttribute('aria-expanded')!=='true')throw new Error('Menu did not expand');
document.querySelector('[role="menuitem"]').click();
if(settings!==1||document.querySelector('[role="menu"]'))throw new Error('Settings action changed');
more.click();
if(!document.body.textContent.includes('Reconnecting…'))throw new Error('Offline narration lost');
window.controlGeometryReady=true;
