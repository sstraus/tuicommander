import { render } from 'solid-js/web';
import { MobileChatScreen } from '../../mobile/screens/MobileChatScreen';
import { Select } from '../../components/ui';
import '../../mobile/mobile.css';
window.fetch=async(input,init)=>{
  const url=typeof input==='string'?input:input.url;
  if(url.endsWith('/config'))return Response.json({ego_executable:'/fixture/ego'});
  if(url.includes('workspace'))return Response.json('/fixture/workspace');
  return Response.json({error:'Fixture connection unavailable'},{status:503});
};
render(() => <section id="geometry" style="height:740px;padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select><MobileChatScreen/>
</section>,document.getElementById('root'));
try {
await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
const field=document.querySelector('textarea');
field.value='Check geometry';field.dispatchEvent(new Event('input',{bubbles:true}));
document.querySelector('[aria-label="Send"]').click();
await new Promise((resolve,reject)=>{
 const end=setTimeout(()=>reject(new Error('Recovery action did not appear')),5000);
 const check=()=>{if([...document.querySelectorAll('button')].some(button=>button.textContent==='Retry')){clearTimeout(end);resolve();}else requestAnimationFrame(check);};check();
});
if(![...document.querySelectorAll('button')].find(button=>button.textContent==='Retry').hasAttribute('data-form-control'))throw new Error('Recovery bypassed shared control');
window.controlGeometryReady=true;
} catch(error){Object.defineProperty(window,'controlGeometryReady',{get(){throw error;}});}
