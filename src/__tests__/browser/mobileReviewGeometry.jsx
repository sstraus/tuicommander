import { render } from 'solid-js/web';
import { ReviewableMarkdown } from '../../mobile/components/ReviewableMarkdown';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
let saved=[];
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <ReviewableMarkdown content="Review this paragraph.\n" imageSrc={path=>path} onCheckboxToggle={()=>{}} onSaveBlockComment={async (comment,range)=>{saved.push({comment,range});return true;}}/>
</section>,document.getElementById('root'));
const open=()=>{
  document.querySelector('[data-comment-source-start]').click();
  [...document.querySelectorAll('button')].find(button=>button.textContent==='Comment').click();
};
try {
await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
open();
let field=document.querySelector('textarea');
let save=[...document.querySelectorAll('button')].find(button=>button.textContent==='Save comment');
if(!save.disabled)throw new Error('Blank comment must not save');
field.value='Keep the scope narrow';field.dispatchEvent(new Event('input',{bubbles:true}));
if(save.disabled)throw new Error('Draft must enable save');
save.click();
await Promise.resolve();await Promise.resolve();
if(saved.length!==1||saved[0].comment.comment!=='Keep the scope narrow'||document.querySelector('textarea'))throw new Error('Save lifecycle changed');
open();
[...document.querySelectorAll('button')].find(button=>button.textContent==='Cancel').click();
if(document.querySelector('textarea'))throw new Error('Cancel did not dismiss');
open();
window.controlGeometryReady=true;
} catch(error) {Object.defineProperty(window,'controlGeometryReady',{get(){throw error;}});}
