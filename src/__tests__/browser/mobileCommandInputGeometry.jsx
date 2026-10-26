import { render } from 'solid-js/web';
import { CommandInput } from '../../mobile/components/CommandInput';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
// Expose the speech capability for layout only; never start recognition or a live PTY.
window.SpeechRecognition ??= class {};
window.fetch = async () => Response.json({});
render(() => <section id="geometry" style="background:var(--bg-secondary);min-height:240px">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <CommandInput sessionId="geometry-only" />
</section>, document.getElementById('root'));
const input=document.querySelector('textarea');
const controls=[...document.querySelectorAll('#geometry button')];
const px=(element,name)=>parseFloat(getComputedStyle(element)[name])||0;
const usable=input.getBoundingClientRect().width-px(input,'paddingLeft')-px(input,'paddingRight')-px(input,'borderLeftWidth')-px(input,'borderRightWidth');
const sizes=controls.map(button=>({name:button.getAttribute('aria-label'),width:button.getBoundingClientRect().width,height:button.getBoundingClientRect().height}));
if(controls.length!==3)throw new Error('Composer must expose menu, dictation and Send');
for(const size of sizes)if(size.width!==(innerWidth<=600?44:36)||size.height!==(innerWidth<=600?44:36))throw new Error(JSON.stringify(size));
if(usable<160)throw new Error(`Composer usable text width below160px: ${usable}`);
const rects=controls.map(button=>button.getBoundingClientRect()).sort((a,b)=>a.left-b.left);
if(rects.some((rect,index)=>index>0&&rect.left<rects[index-1].right))throw new Error('Composer targets overlap');
window.composerGeometry={viewport:innerWidth,usable,sizes};
const report=document.createElement('output');report.textContent=JSON.stringify({viewport:innerWidth,usable,sizes});document.querySelector('#geometry').append(report);
window.controlGeometryReady=true;
