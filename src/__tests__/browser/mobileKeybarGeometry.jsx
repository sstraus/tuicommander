import { render } from 'solid-js/web';
import { TerminalKeybar } from '../../mobile/components/TerminalKeybar';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let slashes=0,keys=[];
render(() => <section id="geometry" style="padding:16px;padding-top:260px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <TerminalKeybar sessionId="geometry" awaitingInput agentType="claude" onSlashRequest={()=>{slashes++;}} onInputKeyRequest={key=>keys.push(key)}/>
  <TerminalKeybar sessionId="ended" sessionExists={false}/>
</section>,document.getElementById('root'));
const buttons=[...document.querySelectorAll('button')];
buttons.find(button=>button.textContent==='/').click();
buttons.find(button=>button.textContent==='Tab').click();
if(slashes!==1||keys.join()!=='Tab')throw new Error('Composer-owned key callbacks changed');
const ctrl=buttons.find(button=>button.textContent==='Ctrl');ctrl.click();
if(ctrl.getAttribute('aria-expanded')!=='true'||document.querySelectorAll('[role="menuitem"]').length!==4)throw new Error('Control menu failed');
document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
if(document.querySelector('[role="menu"]'))throw new Error('Escape did not dismiss menu');
if(buttons.filter(button=>button.disabled).length!==9)throw new Error('Ended controls must remain disabled');
// Catches: keybar readiness depends on desktop-only CSS, leaving PWA borders neutral.
const ready=[...document.querySelectorAll('button[data-ready]')];
const enabled=ready.filter(button=>!button.disabled);
const accent='rgb(89, 168, 221)';
const fail=message=>{window.controlGeometryError=message;throw new Error(message);};
if(enabled.length<2)fail('Fixture must include slash and confirmation readiness');
for(const button of ready){
 const border=getComputedStyle(button).borderTopColor;
 if(!button.disabled && border!==accent)fail(`Ready ${button.textContent} border ${border} is not ${accent}`);
 if(button.disabled && border===accent)fail('Disabled key retains ready accent border');
}
window.controlGeometryReady=true;
