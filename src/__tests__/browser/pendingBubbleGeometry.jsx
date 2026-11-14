import { render } from 'solid-js/web';
import '../../global.css';
import { Transcript } from '../../components/AIChatPanel/Transcript';
const entries = [
  { id:'short',kind:'user',text:'ok',pending:true,queuedCommandId:1 },
  { id:'multi',kind:'user',text:'ok riprendiamo, c\'erano lavori in corso e dobbiamo completare tutte le modifiche. '.repeat(4),pending:true,queuedCommandId:2 },
  { id:'word',kind:'user',text:'A'.repeat(400),pending:true,queuedCommandId:3 },
];
const width = Number(new URLSearchParams(location.search).get('width') || 960);
render(() => <main style={{width:width+'px',height:'840px',display:'flex','flex-direction':'column',margin:'16px'}}><Transcript entries={()=>entries} busy={()=>false} terminalAgent="claude" emptyMessage="Empty" observeToolDuration={false} onSendPendingNow={()=>{}} onCancelPending={()=>{}}/></main>,document.getElementById('root'));
setTimeout(()=>{
 const box=e=>{const r=e.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height};};
 const bubbles=[...document.querySelectorAll('[aria-label="Pending prompt"]')].map(bubble=>{
  const send=bubble.querySelector('[aria-label^="Send now"]');
  const cancel=bubble.querySelector('[aria-label="Cancel pending prompt"]');
  const range=document.createRange();range.selectNodeContents(bubble);range.setEndBefore(send.parentElement);
  const text=[...range.getClientRects()].filter(r=>r.width>0&&r.height>0).map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom}));
  return {box:box(bubble),send:box(send),cancel:box(cancel),textRects:text};
 });
 const result={width,bubbles};
 const output=document.createElement('pre');output.id='geometry-result';output.textContent=JSON.stringify(result);output.style.display='none';document.body.append(output);
},1800);
