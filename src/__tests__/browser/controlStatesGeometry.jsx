import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Textarea } from '../../components/ui/Textarea';
import '../../global.css';
const [invalid,setInvalid]=createSignal(true),[pressed,setPressed]=createSignal(false);
render(() => <section id="geometry" style="padding:16px;--error:rgb(200,10,20);--success:rgb(10,200,20);--accent:rgb(10,20,200);--border:rgb(80,80,80)">
 <div data-row="default"><Input aria-label="Name" aria-invalid={invalid()} aria-describedby="error"/><Select aria-label="Mode" aria-invalid={invalid()} aria-describedby="error"><option>Mode</option></Select><Button>Plain</Button></div>
 <Textarea aria-label="Instructions" aria-invalid={invalid()} aria-describedby="error" />
 <p id="error">Existing validation message</p>
 <Button data-remove-toggle variant="removalToggle" aria-pressed={pressed()} onClick={()=>setPressed(!pressed())}>Integration</Button>
 <Button data-disabled-toggle variant="removalToggle" aria-pressed="true" disabled>Disabled integration</Button>
 <Button data-selection aria-pressed="true">Selected view</Button>
</section>, document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
window.controlStateGeometry=async()=>{
 const controls=[...document.querySelectorAll('input,select,textarea')];
 const read=element=>({border:getComputedStyle(element).borderColor,outline:getComputedStyle(element).outlineColor,outlineStyle:getComputedStyle(element).outlineStyle,height:element.getBoundingClientRect().height});
 const invalid=controls.map(read);
 const focused=controls.map(element=>{element.focus();return read(element);});
 controls.at(-1).blur();setInvalid(false);await frame();const valid=controls.map(read);
 const button=document.querySelector('[data-remove-toggle]');const off=read(button);button.click();await frame();const on=read(button);
 const disabled=read(document.querySelector('[data-disabled-toggle]'));
 const selection=read(document.querySelector('[data-selection]'));
 return {invalid,focused,valid,off,on,disabled,selection};
};
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
