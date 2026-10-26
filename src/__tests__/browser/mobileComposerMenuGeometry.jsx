import { render } from 'solid-js/web';
import { MobileComposerMenu } from '../../mobile/components/MobileComposerMenu';
import { CommandInput } from '../../mobile/components/CommandInput';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let queued=0;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <div id="menu-case" style="position:relative;margin-top:300px"><MobileComposerMenu onAttach={()=>{}} onPark={()=>{}} onQueue={()=>{queued++;}} onStop={()=>{}} onCommands={()=>{}}/></div>
  <div id="terminal-case"><CommandInput sessionId="geometry-only"/></div>
</section>, document.getElementById('root'));
const trigger=document.querySelector('#menu-case button');
trigger.click();
if(trigger.getAttribute('aria-expanded')!=='true')throw new Error('Menu did not open');
document.querySelector('#menu-case [role="menuitem"]').focus();
document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
if(document.activeElement!==trigger || document.querySelector('#menu-case [role="menu"]'))throw new Error('Escape must close and restore trigger focus');
trigger.click();
[...document.querySelectorAll('#menu-case [role="menuitem"]')].find(item=>item.textContent.trim()==='Queue').click();
if(queued!==1 || trigger.getAttribute('aria-expanded')!=='false')throw new Error('Queue must invoke once and dismiss');
trigger.click();
window.controlGeometryReady=true;
