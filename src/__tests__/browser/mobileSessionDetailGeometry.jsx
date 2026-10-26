import { render } from 'solid-js/web';
import { SessionDetailScreen } from '../../mobile/screens/SessionDetailScreen';
import '../../mobile/mobile.css';
window.fetch=async(input)=>{
 const url=typeof input==='string'?input:input.url;
 if(url.includes('markdown'))return Response.json([]);
 if(url.endsWith('/config'))return Response.json({});
 return Response.json({error:'Fixture unavailable'},{status:404});
};
let backed=0,files=0;
render(() => <section id="geometry" style="position:relative;height:760px;background:var(--bg-secondary)">
 <SessionDetailScreen session={{session_id:'geometry-ended',display_name:'Geometry session',cwd:'/work/project',worktree_path:null,worktree_branch:null,state:{awaiting_input:false,rate_limited:false,last_activity_ms:0}}} sessionExists={false} subscribeChatWake={()=>()=>{}} onBack={()=>{backed++;}} onOpenFiles={()=>{files++;}}/>
</section>,document.getElementById('root'));
try {
const named=name=>[...document.querySelectorAll('button')].find(button=>button.getAttribute('aria-label')===name||button.textContent.trim()===name);
named('Back to sessions').click();
if(backed!==1)throw new Error('Back navigation changed');
named('More session actions').click();named('Files').click();
if(files!==1)throw new Error('Files navigation changed');
named('More session actions').click();named('Search output').click();
const input=document.querySelector('input[placeholder="Filter output..."]');
input.value='needle';input.dispatchEvent(new Event('input',{bubbles:true}));
const clear=input.parentElement.querySelector('button');clear.click();
if(input.value!=='')throw new Error('Search clear changed');
input.value='geometry';input.dispatchEvent(new Event('input',{bubbles:true}));
input.parentElement.dataset.row='default';
window.controlGeometryReady=true;
} catch(error){Object.defineProperty(window,'controlGeometryReady',{get(){throw error;}});}
