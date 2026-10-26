import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import { Textarea } from '../../components/ui/Textarea';
import '../../global.css';
const [value,setValue] = createSignal('Initial\nmultiline\ndraft');
let field;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
 <div data-row="default"><Button>Default</Button><Button disabled>Disabled</Button></div>
 <Textarea autoGrow aria-label="Commit message" ref={node => {field=node;}} value={value()} onInput={event=>setValue(event.currentTarget.value)} style="width:100%;margin-top:16px" />
</section>,document.getElementById('root'));
const frame=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
// Catches initial mount, input growth, controlled replacement and failure to shrink after clearing.
window.textareaGeometry = async () => {
 const read=()=>({height:field.getBoundingClientRect().height,scroll:field.scrollHeight,client:field.clientHeight,overflow:getComputedStyle(field).overflowY});
 const initial=read();
 field.value=Array(30).fill('Long commit message').join('\n');field.dispatchEvent(new InputEvent('input',{bubbles:true}));await frame();
 const long=read();
 setValue('');await frame();const cleared=read();
 setValue('one\ntwo\nthree');await frame();const replaced=read();
 setValue('A commit message with enough words to wrap across narrow widths.');await frame();const wide=read();
 field.style.width='100px';await frame();const narrow=read();
 field.style.width='100%';await frame();
 return {initial,long,cleared,replaced,wide,narrow};
};
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
