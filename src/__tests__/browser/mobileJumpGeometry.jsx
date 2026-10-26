import { render } from 'solid-js/web';
import { MobileJumpBottom } from '../../mobile/components/MobileJumpBottom';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let jumped=0;
render(() => <section id="geometry" style="position:relative;height:220px;padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <MobileJumpBottom visible onScrollToBottom={()=>{jumped++;}}/>
  <MobileJumpBottom visible={false} onScrollToBottom={()=>{throw new Error('Hidden action invoked');}}/>
</section>, document.getElementById('root'));
const buttons=document.querySelectorAll('button[aria-label="Scroll to bottom"]');
if(buttons.length!==1)throw new Error('Hidden jump action must not mount');
const button=buttons[0];
const mouse=new MouseEvent('mousedown',{bubbles:true,cancelable:true});
button.dispatchEvent(mouse);
if(!mouse.defaultPrevented)throw new Error('Jump must not steal composer focus');
button.click();
if(jumped!==1)throw new Error('Jump callback changed');
const css=getComputedStyle(button);
if(css.right!=='12px'||css.bottom!=='8px')throw new Error('Floating position changed');
window.controlGeometryReady=true;
