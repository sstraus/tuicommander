import { render } from 'solid-js/web';
import { NewSessionSheet } from '../../mobile/components/NewSessionSheet';
import '../../mobile/mobile.css';
const original=window.fetch.bind(window);
window.fetch=(input,init)=>{
  const url=typeof input==='string'?input:input.url;
  if(url.includes('/agents/detect-all'))return Promise.resolve(Response.json({claude:{path:'/fixture/claude'}}));
  throw new Error(`Unexpected fixture request: ${url}`);
};
let dismissed=0;
render(() => <section id="geometry" style="position:relative;height:800px;background:var(--bg-secondary)">
  <NewSessionSheet repos={['/work/project-alpha','/work/a-very-long-parent-directory/project-beta']} onDismiss={()=>{dismissed++;}}/>
</section>,document.getElementById('root'));
await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
const input=document.querySelector('input[type="search"]');
input.value='beta';input.dispatchEvent(new Event('input',{bubbles:true}));
if(document.querySelectorAll('button[data-control-layout="row"]').length!==1)throw new Error('Repository filter changed');
input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));
const rows=[...document.querySelectorAll('button[data-control-layout="row"]')];
if(rows.length!==2 || rows.some(row=>row.type!=='button'||row.disabled||row.getBoundingClientRect().height<44))throw new Error('Native repository rows changed');
document.querySelector('button[aria-label="Close new session"]').click();
if(dismissed!==1)throw new Error('Close callback changed');
window.fetch=original;
window.controlGeometryReady=true;
